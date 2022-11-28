// Copyright 2018-2022 Parity Technologies (UK) Ltd.
// This file is part of cargo-contract.
//
// cargo-contract is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// cargo-contract is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with cargo-contract.  If not, see <http://www.gnu.org/licenses/>.

#![deny(unused_crate_dependencies)]

mod cmd;

use self::cmd::{
    BuildCommand,
    CallCommand,
    CheckCommand,
    DecodeCommand,
    ErrorVariant,
    InstantiateCommand,
    TestCommand,
    UploadCommand,
};
use contract_build::{
    metadata::MetadataResult,
    name_value_println,
    util::DEFAULT_KEY_COL_WIDTH,
    ManifestPath,
    OutputType,
};
use std::{
    convert::TryFrom,
    fmt::{
        Display,
        Formatter,
        Result as DisplayResult,
    },
    path::PathBuf,
    str::FromStr,
};

use ::wasm_opt::OptimizationOptions;
use anyhow::{
    anyhow,
    Error,
    Result,
};
use clap::{
    Args,
    Parser,
    Subcommand,
};
use colored::Colorize;

// These crates are only used when we run integration tests `--features integration-tests`. However
// since we can't have optional `dev-dependencies` we pretend to use them during normal test runs
// in order to satisfy the `unused_crate_dependencies` lint.
#[cfg(test)]
use assert_cmd as _;

#[cfg(test)]
use predicates as _;

#[cfg(test)]
use regex as _;

// Only used on windows.
use which as _;

#[derive(Debug, Parser)]
#[clap(bin_name = "cargo")]
#[clap(version = env!("CARGO_CONTRACT_CLI_IMPL_VERSION"))]
pub(crate) enum Opts {
    /// Utilities to develop Wasm smart contracts.
    #[clap(name = "contract")]
    #[clap(version = env!("CARGO_CONTRACT_CLI_IMPL_VERSION"))]
    #[clap(action = ArgAction::DeriveDisplayOrder)]
    Contract(ContractArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ContractArgs {
    #[clap(subcommand)]
    cmd: Command,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct HexData(pub Vec<u8>);

impl FromStr for HexData {
    type Err = hex::FromHexError;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        hex::decode(input).map(HexData)
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Setup and create a new smart contract project
    #[clap(name = "new")]
    New {
        /// The name of the newly created smart contract
        name: String,
        /// The optional target directory for the contract project
        #[clap(short, long, value_parser)]
        target_dir: Option<PathBuf>,
    },
    /// Compiles the contract, generates metadata, bundles both together in a `<name>.contract` file
    #[clap(name = "build")]
    Build(BuildCommand),
    /// Check that the code builds as Wasm; does not output any `<name>.contract` artifact to the `target/` directory
    #[clap(name = "check")]
    Check(CheckCommand),
    /// Test the smart contract off-chain
    #[clap(name = "test")]
    Test(TestCommand),
    /// Upload contract code
    #[clap(name = "upload")]
    Upload(UploadCommand),
    /// Instantiate a contract
    #[clap(name = "instantiate")]
    Instantiate(InstantiateCommand),
    /// Call a contract
    #[clap(name = "call")]
    Call(CallCommand),
    /// Decodes a contracts input or output data (supplied in hex-encoding)
    #[clap(name = "decode")]
    Decode(DecodeCommand),
}

fn main() {
    tracing_subscriber::fmt::init();

    let Opts::Contract(args) = Opts::parse();

    match exec(args.cmd) {
        Ok(()) => {}
        Err(err) => {
            eprintln!("{}", err);
            std::process::exit(1);
        }
    }
}

fn exec(cmd: Command) -> Result<()> {
    match &cmd {
        Command::New { name, target_dir } => {
            contract_build::new_contract_project(name, target_dir.as_ref())?;
            println!("Created contract {}", name);
            Ok(())
        }
        Command::Build(build) => {
            let result = build.exec().map_err(format_err)?;

            if matches!(result.output_type, OutputType::Json) {
                println!("{}", result.serialize_json()?)
            } else if result.verbosity.is_verbose() {
                println!("{}", result.display())
            }
            Ok(())
        }
        Command::Check(check) => {
            let res = check.exec().map_err(format_err)?;
            assert!(
                res.dest_wasm.is_none(),
                "no dest_wasm must be on the generation result"
            );
            if res.verbosity.is_verbose() {
                println!("\nYour contract's code was built successfully.")
            }
            Ok(())
        }
        Command::Test(test) => {
            let res = test.exec().map_err(format_err)?;
            if res.verbosity.is_verbose() {
                println!("{}", res.display()?)
            }
            Ok(())
        }
        Command::Upload(upload) => {
            upload
                .run()
                .map_err(|err| map_extrinsic_err(err, upload.is_json()))
        }
        Command::Instantiate(instantiate) => {
            instantiate
                .run()
                .map_err(|err| map_extrinsic_err(err, instantiate.is_json()))
        }
        Command::Call(call) => {
            call.run()
                .map_err(|err| map_extrinsic_err(err, call.is_json()))
        }
        Command::Decode(decode) => decode.run().map_err(format_err),
    }
}

fn map_extrinsic_err(err: ErrorVariant, is_json: bool) -> Error {
    if is_json {
        anyhow!(
            "{}",
            serde_json::to_string_pretty(&err)
                .expect("error serialization is infallible; qed")
        )
    } else {
        format_err(err)
    }
}

fn format_err<E: Display>(err: E) -> Error {
    anyhow!(
        "{} {}",
        "ERROR:".bright_red().bold(),
        format!("{}", err).bright_red()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_result_seralization_sanity_check() {
        // given
        let raw_result = r#"{
  "dest_wasm": "/path/to/contract.wasm",
  "metadata_result": {
    "dest_metadata": "/path/to/metadata.json",
    "dest_bundle": "/path/to/contract.contract"
  },
  "target_directory": "/path/to/target",
  "optimization_result": {
    "dest_wasm": "/path/to/contract.wasm",
    "original_size": 64.0,
    "optimized_size": 32.0
  },
  "build_mode": "Debug",
  "build_artifact": "All",
  "verbosity": "Quiet"
}"#;

        let build_result = BuildResult {
            dest_wasm: Some(PathBuf::from("/path/to/contract.wasm")),
            metadata_result: Some(MetadataResult {
                dest_metadata: PathBuf::from("/path/to/metadata.json"),
                dest_bundle: PathBuf::from("/path/to/contract.contract"),
            }),
            target_directory: PathBuf::from("/path/to/target"),
            optimization_result: Some(OptimizationResult {
                dest_wasm: PathBuf::from("/path/to/contract.wasm"),
                original_size: 64.0,
                optimized_size: 32.0,
            }),
            build_mode: Default::default(),
            build_artifact: Default::default(),
            verbosity: Verbosity::Quiet,
            output_type: OutputType::Json,
        };

        // when
        let serialized_result = build_result.serialize_json();

        // then
        assert!(serialized_result.is_ok());
        assert_eq!(serialized_result.unwrap(), raw_result);
    }
}

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
mod config;

use self::cmd::{
    BuildCommand,
    CallCommand,
    CheckCommand,
    DecodeCommand,
    ErrorVariant,
    GenerateSchemaCommand,
    InfoCommand,
    InstantiateCommand,
    RemoveCommand,
    RpcCommand,
    StorageCommand,
    UploadCommand,
    VerifyCommand,
    VerifySchemaCommand,
};
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
use cmd::encode::EncodeCommand;
use colored::Colorize;
use config::{
    PolkadotBaseConfig, SignerConfig,
};
use contract_build::{
    util::DEFAULT_KEY_COL_WIDTH,
    OutputType,
};
use contract_extrinsics::InstantiateExec;
use ink_env::Environment;
use serde::Serialize;
use sp_weights::Weight;
use std::{
    fmt::{
        Debug,
        Display,
    },
    path::PathBuf,
    str::FromStr,
};
use subxt::{
    ext::{
        codec::Decode,
        scale_decode::IntoVisitor, scale_encode::EncodeAsType,
    },
    Config,
};
use tokio::runtime::Runtime;
// These crates are only used when we run integration tests `--features
// integration-tests`. However since we can't have optional `dev-dependencies` we pretend
// to use them during normal test runs in order to satisfy the `unused_crate_dependencies`
// lint.
#[cfg(test)]
use assert_cmd as _;
#[cfg(test)]
use predicates as _;
#[cfg(test)]
use regex as _;
#[cfg(test)]
use tempfile as _;

// Only used on windows.
use which as _;

#[derive(Debug, Parser)]
#[clap(bin_name = "cargo")]
#[clap(version = env!("CARGO_CONTRACT_CLI_IMPL_VERSION"))]
pub(crate) enum Opts<C: Config + Environment  + SignerConfig<C>>
where
    <C as Config>::AccountId: Sync + Send + FromStr,
    <<C as Config>::AccountId as FromStr>::Err:
        Into<Box<(dyn std::error::Error + Send + Sync)>>,
    <C as Environment>::Balance: Sync + Send + From<u128> + Debug + FromStr,
    <<C as Environment>::Balance as FromStr>::Err:
        Into<Box<(dyn std::error::Error + Send + Sync)>>,
{
    /// Utilities to develop Wasm smart contracts.
    #[clap(name = "contract")]
    #[clap(version = env!("CARGO_CONTRACT_CLI_IMPL_VERSION"))]
    #[clap(action = ArgAction::DeriveDisplayOrder)]
    Contract(ContractArgs<C>),
}

#[derive(Debug, Args)]
pub(crate) struct ContractArgs<C: Config + Environment  + SignerConfig<C>>
where
    <C as Config>::AccountId: Sync + Send + FromStr,
    <<C as Config>::AccountId as FromStr>::Err:
        Into<Box<(dyn std::error::Error + Send + Sync)>>,
    <C as Environment>::Balance: Sync + Send + From<u128> + Debug + FromStr,
    <<C as Environment>::Balance as FromStr>::Err:
        Into<Box<(dyn std::error::Error + Send + Sync)>>,

{
    #[clap(subcommand)]
    cmd: Command<C>,
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
enum Command<C: Config + Environment + SignerConfig<C>>
where
    <C as Config>::AccountId: Sync + Send + FromStr,
    <<C as Config>::AccountId as FromStr>::Err:
        Into<Box<(dyn std::error::Error + Send + Sync)>>,
    <C as Environment>::Balance: Sync + Send + From<u128> + Debug + FromStr,
    <<C as Environment>::Balance as FromStr>::Err:
        Into<Box<(dyn std::error::Error + Send + Sync)>>,
{
    /// Setup and create a new smart contract project
    #[clap(name = "new")]
    New {
        /// The name of the newly created smart contract
        name: String,
        /// The optional target directory for the contract project
        #[clap(short, long, value_parser)]
        target_dir: Option<PathBuf>,
    },
    /// Compiles the contract, generates metadata, bundles both together in a
    /// `<name>.contract` file
    #[clap(name = "build")]
    Build(BuildCommand),
    /// Check that the code builds as Wasm; does not output any `<name>.contract`
    /// artifact to the `target/` directory
    #[clap(name = "check")]
    Check(CheckCommand),
    /// Upload contract code
    #[clap(name = "upload")]
    Upload(UploadCommand),
    /// Instantiate a contract
    #[clap(name = "instantiate")]
    Instantiate(InstantiateCommand),
    /// Call a contract
    #[clap(name = "call")]
    Call(CallCommand<C>),
    /// Encodes a contracts input calls and their arguments
    #[clap(name = "encode")]
    Encode(EncodeCommand),
    /// Decodes a contracts input or output data (supplied in hex-encoding)
    #[clap(name = "decode")]
    Decode(DecodeCommand),
    /// Remove contract code
    #[clap(name = "remove")]
    Remove(RemoveCommand),
    /// Display information about a contract
    #[clap(name = "info")]
    Info(InfoCommand<C>),
    /// Inspect the on-chain storage of a contract.
    #[clap(name = "storage")]
    Storage(StorageCommand<C>),
    /// Verifies that a given contract binary matches the build result of the specified
    /// workspace.
    #[clap(name = "verify")]
    Verify(VerifyCommand),
    /// Generates schema from the current metadata specification.
    #[clap(name = "generate-schema")]
    GenerateSchema(GenerateSchemaCommand),
    /// Verify schema from the current metadata specification.
    #[clap(name = "verify-schema")]
    VerifySchema(VerifySchemaCommand),
    /// Make a raw RPC call.
    #[clap(name = "rpc")]
    Rpc(RpcCommand),
}

fn main() {
    tracing_subscriber::fmt::init();
    let config = config::select_config("eth");

    // let Opts::Contract(args) = match config {
    //     ChainConfig::EthereumBaseConfig =>  Opts::<EthereumBaseConfig>::parse(),
    //     ChainConfig::PolkadotBaseConfig =>  Opts::<PolkadotBaseConfig>::parse(),
    // };

    let Opts::Contract(args) = Opts::<PolkadotBaseConfig>::parse();

    match exec(args.cmd) {
        Ok(()) => {}
        Err(err) => {
            eprintln!("{err:?}");
            std::process::exit(1);
        }
    }
}

fn exec<C: Config + Environment  + SignerConfig<C>>(cmd: Command<C>) -> Result<()>
where
    <C as Config>::AccountId:
        Serialize + Display + IntoVisitor + Decode + AsRef<[u8]> + Send + Sync + FromStr + EncodeAsType,
    <<C as Config>::AccountId as FromStr>::Err:
        Into<Box<(dyn std::error::Error + Send + Sync)>>,
    <C as Config>::Hash: IntoVisitor + Display,
    <C as Environment>::Balance: Serialize + Decode + Debug + IntoVisitor + Sync + Send + From<u128> + Default + Display  + FromStr,
    <<C as Environment>::Balance as FromStr>::Err:
        Into<Box<(dyn std::error::Error + Send + Sync)>>,
    <<C as Config>::ExtrinsicParams as subxt::config::ExtrinsicParams<C>>::OtherParams: Default,

{
    let runtime = Runtime::new().expect("Failed to create Tokio runtime");
    match &cmd {
        Command::New { name, target_dir } => {
            contract_build::new_contract_project(name, target_dir.as_ref())?;
            println!("Created contract {name}");
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
            Ok(())
        }
        Command::Upload(upload) => {
            runtime.block_on(async {
                upload
                    .handle()
                    .await
                    .map_err(|err| map_extrinsic_err(err, upload.output_json()))
            })
        }
        Command::Instantiate(instantiate) => {
            runtime.block_on(async {
                instantiate
                    .handle()
                    .await
                    .map_err(|err| map_extrinsic_err(err, instantiate.output_json()))
            })
        }
        Command::Call(call) => {
            runtime.block_on(async {
                call.handle()
                    .await
                    .map_err(|err| map_extrinsic_err(err, call.output_json()))
            })
        }
        Command::Encode(encode) => encode.run().map_err(format_err),
        Command::Decode(decode) => decode.run().map_err(format_err),
        Command::Remove(remove) => {
            runtime.block_on(async {
                remove
                    .handle()
                    .await
                    .map_err(|err| map_extrinsic_err(err, remove.output_json()))
            })
        }
        Command::Info(info) => {
            runtime.block_on(async { info.run().await.map_err(format_err) })
        }
        Command::Storage(storage) => {
            runtime.block_on(async { storage.run().await.map_err(format_err) })
        }
        Command::Verify(verify) => {
            let result = verify.run().map_err(format_err)?;

            if result.output_json {
                println!("{}", result.serialize_json()?)
            } else if result.verbosity.is_verbose() {
                println!("{}", result.display())
            }
            Ok(())
        }
        Command::GenerateSchema(generate) => {
            let result = generate.run().map_err(format_err)?;
            println!("{}", result);
            Ok(())
        }
        Command::VerifySchema(verify) => {
            let result = verify.run().map_err(format_err)?;

            if result.output_json {
                println!("{}", result.serialize_json()?)
            } else if result.verbosity.is_verbose() {
                println!("{}", result.display())
            }
            Ok(())
        }
        Command::Rpc(rpc) => {
            runtime.block_on(async { rpc.run().await.map_err(format_err) })
        }
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

fn format_err<E: Debug>(err: E) -> Error {
    anyhow!(
        "{} {}",
        "ERROR:".bright_red().bold(),
        format!("{err:?}").bright_red()
    )
}

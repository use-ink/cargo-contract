// Copyright 2018-2020 Parity Technologies (UK) Ltd.
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

mod cmd;
mod crate_metadata;
mod util;
mod workspace;

use self::workspace::ManifestPath;

#[cfg(feature = "extrinsics")]
use sp_core::{crypto::Pair, sr25519, H256};
use std::{
    convert::{TryFrom, TryInto},
    path::PathBuf,
};
#[cfg(feature = "extrinsics")]
use subxt::PairSigner;

use anyhow::{Error, Result};
use colored::Colorize;
use structopt::{clap, StructOpt};

#[derive(Debug, StructOpt)]
#[structopt(bin_name = "cargo")]
pub(crate) enum Opts {
    /// Utilities to develop Wasm smart contracts.
    #[structopt(name = "contract")]
    #[structopt(setting = clap::AppSettings::UnifiedHelpMessage)]
    #[structopt(setting = clap::AppSettings::DeriveDisplayOrder)]
    #[structopt(setting = clap::AppSettings::DontCollapseArgsInUsage)]
    Contract(ContractArgs),
}

#[derive(Debug, StructOpt)]
pub(crate) struct ContractArgs {
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct HexData(pub Vec<u8>);

#[cfg(feature = "extrinsics")]
impl std::str::FromStr for HexData {
    type Err = hex::FromHexError;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        hex::decode(input).map(HexData)
    }
}

/// Arguments required for creating and sending an extrinsic to a substrate node
#[cfg(feature = "extrinsics")]
#[derive(Debug, StructOpt)]
pub(crate) struct ExtrinsicOpts {
    /// Websockets url of a substrate node
    #[structopt(
        name = "url",
        long,
        parse(try_from_str),
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
    /// Secret key URI for the account deploying the contract.
    #[structopt(name = "suri", long, short)]
    suri: String,
    /// Password for the secret key
    #[structopt(name = "password", long, short)]
    password: Option<String>,
}

#[cfg(feature = "extrinsics")]
impl ExtrinsicOpts {
    pub fn signer(&self) -> Result<PairSigner<subxt::DefaultNodeRuntime, sr25519::Pair>> {
        let pair =
            sr25519::Pair::from_string(&self.suri, self.password.as_ref().map(String::as_ref))
                .map_err(|_| anyhow::anyhow!("Secret string error"))?;
        Ok(PairSigner::new(pair))
    }
}

#[derive(Debug, StructOpt)]
struct VerbosityFlags {
    #[structopt(long)]
    quiet: bool,
    #[structopt(long)]
    verbose: bool,
}

#[derive(Clone, Copy)]
enum Verbosity {
    Quiet,
    Verbose,
}

impl TryFrom<&VerbosityFlags> for Option<Verbosity> {
    type Error = Error;

    fn try_from(value: &VerbosityFlags) -> Result<Self, Self::Error> {
        match (value.quiet, value.verbose) {
            (false, false) => Ok(None),
            (true, false) => Ok(Some(Verbosity::Quiet)),
            (false, true) => Ok(Some(Verbosity::Verbose)),
            (true, true) => anyhow::bail!("Cannot pass both --quiet and --verbose flags"),
        }
    }
}

#[derive(Debug, StructOpt)]
struct UnstableOptions {
    /// Use the original manifest (Cargo.toml), do not modify for build optimizations
    #[structopt(long = "unstable-options", short = "Z", number_of_values = 1)]
    options: Vec<String>,
}

#[derive(Clone, Default)]
struct UnstableFlags {
    original_manifest: bool,
}

impl TryFrom<&UnstableOptions> for UnstableFlags {
    type Error = Error;

    fn try_from(value: &UnstableOptions) -> Result<Self, Self::Error> {
        let valid_flags = ["original-manifest"];
        let invalid_flags = value
            .options
            .iter()
            .filter(|o| !valid_flags.contains(&o.as_str()))
            .collect::<Vec<_>>();
        if !invalid_flags.is_empty() {
            anyhow::bail!("Unknown unstable-options {:?}", invalid_flags)
        }
        Ok(UnstableFlags {
            original_manifest: value.options.contains(&"original-manifest".to_owned()),
        })
    }
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Setup and create a new smart contract project
    #[structopt(name = "new")]
    New {
        /// The name of the newly created smart contract
        name: String,
        /// The optional target directory for the contract project
        #[structopt(short, long, parse(from_os_str))]
        target_dir: Option<PathBuf>,
    },
    /// Compiles the contract, generates metadata, bundles both together in a '.contract' file
    #[structopt(name = "build")]
    Build {
        /// Path to the Cargo.toml of the contract to build
        #[structopt(long, parse(from_os_str))]
        manifest_path: Option<PathBuf>,
        /// Only the Wasm and the metadata are generated, no bundled .contract file is created
        #[structopt(long = "skip-bundle", conflicts_with = "skip-metadata")]
        skip_bundle: bool,
        /// Only the Wasm is created, generation of metadata and a bundled .contract file is skipped
        #[structopt(long = "skip-metadata", conflicts_with = "skip-bundle")]
        skip_metadata: bool,
        #[structopt(flatten)]
        verbosity: VerbosityFlags,
        #[structopt(flatten)]
        unstable_options: UnstableOptions,
    },
    /// Command has been deprecated, use 'cargo contract build' instead
    #[structopt(name = "generate-metadata")]
    GenerateMetadata {
        /// Path to the Cargo.toml of the contract to build
        #[structopt(long, parse(from_os_str))]
        manifest_path: Option<PathBuf>,
        #[structopt(flatten)]
        verbosity: VerbosityFlags,
        #[structopt(flatten)]
        unstable_options: UnstableOptions,
    },
    /// Check that the Wasm builds; does not optimize, generate metadata, or bundle
    #[structopt(name = "check")]
    Check {
        /// Path to the Cargo.toml of the contract to build
        #[structopt(long, parse(from_os_str))]
        manifest_path: Option<PathBuf>,
        #[structopt(flatten)]
        verbosity: VerbosityFlags,
        #[structopt(flatten)]
        unstable_options: UnstableOptions,
    },
    /// Test the smart contract off-chain
    #[structopt(name = "test")]
    Test {},
    /// Upload the smart contract code to the chain
    #[cfg(feature = "extrinsics")]
    #[structopt(name = "deploy")]
    Deploy {
        #[structopt(flatten)]
        extrinsic_opts: ExtrinsicOpts,
        /// Path to wasm contract code, defaults to ./target/<name>-pruned.wasm
        #[structopt(parse(from_os_str))]
        wasm_path: Option<PathBuf>,
    },
    /// Instantiate a deployed smart contract
    #[cfg(feature = "extrinsics")]
    #[structopt(name = "instantiate")]
    Instantiate {
        #[structopt(flatten)]
        extrinsic_opts: ExtrinsicOpts,
        /// Transfers an initial balance to the instantiated contract
        #[structopt(name = "endowment", long, default_value = "0")]
        endowment: u128,
        /// Maximum amount of gas to be used for this command
        #[structopt(name = "gas", long, default_value = "500000000")]
        gas_limit: u64,
        /// The hash of the smart contract code already uploaded to the chain
        #[structopt(long, parse(try_from_str = parse_code_hash))]
        code_hash: H256,
        /// Hex encoded data to call a contract constructor
        #[structopt(long)]
        data: HexData,
    },
}

#[cfg(feature = "extrinsics")]
fn parse_code_hash(input: &str) -> Result<H256> {
    let bytes = hex::decode(input)?;
    if bytes.len() != 32 {
        anyhow::bail!("Code hash should be 32 bytes in length")
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(H256(arr))
}

fn main() {
    env_logger::init();

    let Opts::Contract(args) = Opts::from_args();
    match exec(args.cmd) {
        Ok(msg) => println!("\t{}", msg),
        Err(err) => eprintln!(
            "{} {}",
            "ERROR:".bright_red().bold(),
            format!("{:?}", err).bright_red()
        ),
    }
}

fn exec(cmd: Command) -> Result<String> {
    match &cmd {
        Command::New { name, target_dir } => cmd::new::execute(name, target_dir.as_ref()),
        Command::Build {
            manifest_path,
            verbosity,
            skip_bundle,
            skip_metadata,
            unstable_options,
        } => {
            if *(skip_metadata) {
                let manifest_path = ManifestPath::try_from(manifest_path.as_ref())?;
                let dest_wasm = cmd::build::execute(
                    &manifest_path,
                    verbosity.try_into()?,
                    unstable_options.try_into()?,
                    true,
                )?;
                return Ok(format!(
                    "\nYour contract's code is ready. You can find it here:\n{}",
                    dest_wasm.display().to_string().bold()
                ));
            }
            let manifest_path = ManifestPath::try_from(manifest_path.as_ref())?;
            let metadata_result = cmd::metadata::execute(
                &manifest_path,
                verbosity.try_into()?,
                false,
                unstable_options.try_into()?,
            )?;
            if *(skip_bundle) {
                return Ok(format!(
                    "\nYour contract's code is ready. You can find it here:\n{}
                    \nYour contract's metadata is ready. You can find it here:\n{}",
                    metadata_result.wasm_file.display().to_string().bold(),
                    metadata_result.metadata_file.display().to_string().bold(),
                ));
            }
            let bundle_result = cmd::metadata::execute(
                &manifest_path,
                verbosity.try_into()?,
                true,
                unstable_options.try_into()?,
            )?;
            Ok(format!(
                "\nYour contract's code is ready. You can find it here:\n{}
                \nYour contract's metadata is ready. You can find it here:\n{}
                \nYour contract bundle (code + metadata) is ready. You can find it here:\n{}",
                bundle_result.wasm_file.display().to_string().bold(),
                metadata_result.metadata_file.display().to_string().bold(),
                bundle_result.metadata_file.display().to_string().bold()
            ))
        }
        Command::Check {
            manifest_path,
            verbosity,
            unstable_options,
        } => {
            let manifest_path = ManifestPath::try_from(manifest_path.as_ref())?;
            let _dest_unoptimized_wasm = cmd::build::execute(
                &manifest_path,
                verbosity.try_into()?,
                unstable_options.try_into()?,
                false,
            )?;
            Ok(format!("\nYour contract's code was built successfully."))
        }
        Command::GenerateMetadata {
            manifest_path: _,
            verbosity: _,
            unstable_options: _,
        } => Err(anyhow::anyhow!(format!(
            "Command deprecated, use 'cargo contract build' instead"
        ))),
        Command::Test {} => Err(anyhow::anyhow!("Command unimplemented")),
        #[cfg(feature = "extrinsics")]
        Command::Deploy {
            extrinsic_opts,
            wasm_path,
        } => {
            let code_hash = cmd::execute_deploy(extrinsic_opts, wasm_path.as_ref())?;
            Ok(format!("Code hash: {:?}", code_hash))
        }
        #[cfg(feature = "extrinsics")]
        Command::Instantiate {
            extrinsic_opts,
            endowment,
            code_hash,
            gas_limit,
            data,
        } => {
            let contract_account = cmd::execute_instantiate(
                extrinsic_opts,
                *endowment,
                *gas_limit,
                *code_hash,
                data.clone(),
            )?;
            Ok(format!("Contract account: {:?}", contract_account))
        }
    }
}

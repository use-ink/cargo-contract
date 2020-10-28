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
#[cfg(test)]
#[cfg(feature = "integration-tests")]
mod tests;
mod util;
mod workspace;

use std::{
    convert::{TryFrom, TryInto},
    path::PathBuf,
    process,
};

use anyhow::{Error, Result};
use colored::Colorize;
use structopt::{clap, StructOpt};

#[cfg(feature = "extrinsics")]
use crate::cmd::{DeployCommand, CallCommand, InstantiateCommand};
#[cfg(feature = "extrinsics")]
use sp_core::{crypto::Pair, sr25519};
#[cfg(feature = "extrinsics")]
use subxt::PairSigner;

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
#[derive(Clone, Debug, StructOpt)]
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
    #[structopt(flatten)]
    verbosity: VerbosityFlags,
}

#[cfg(feature = "extrinsics")]
impl ExtrinsicOpts {
    pub fn signer(&self) -> Result<PairSigner<subxt::ContractsTemplateRuntime, sr25519::Pair>> {
        let pair =
            sr25519::Pair::from_string(&self.suri, self.password.as_ref().map(String::as_ref))
                .map_err(|_| anyhow::anyhow!("Secret string error"))?;
        Ok(PairSigner::new(pair))
    }

    /// Returns the verbosity
    pub fn verbosity(&self) -> Result<Verbosity> {
        self.verbosity.try_into()
    }
}

#[derive(Clone, Copy, Debug, StructOpt)]
struct VerbosityFlags {
    #[structopt(long)]
    quiet: bool,
    #[structopt(long)]
    verbose: bool,
}

impl Default for VerbosityFlags {
    fn default() -> Self {
        Self::quiet()
    }
}

impl VerbosityFlags {
    pub fn quiet() -> Self {
        Self { quiet: true, verbose: false }
    }
}

#[derive(Clone, Copy)]
pub enum Verbosity {
    Quiet,
    Verbose,
    NotSpecified,
}

impl Default for Verbosity {
    fn default() -> Self {
        Self::NotSpecified
    }
}

impl TryFrom<VerbosityFlags> for Verbosity {
    type Error = Error;

    fn try_from(value: VerbosityFlags) -> Result<Self, Self::Error> {
        match (value.quiet, value.verbose) {
            (false, false) => Ok(Verbosity::NotSpecified),
            (true, false) => Ok(Verbosity::Quiet),
            (false, true) => Ok(Verbosity::Verbose),
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
    /// Compiles the smart contract
    #[structopt(name = "build")]
    Build {
        #[structopt(flatten)]
        verbosity: VerbosityFlags,
        #[structopt(flatten)]
        unstable_options: UnstableOptions,
    },
    /// Generate contract metadata artifacts
    #[structopt(name = "generate-metadata")]
    GenerateMetadata {
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
    Deploy(DeployCommand),
    /// Instantiate a deployed smart contract
    #[cfg(feature = "extrinsics")]
    Instantiate(InstantiateCommand),
    #[cfg(feature = "extrinsics")]
    Call(CallCommand),
}

fn main() {
    env_logger::init();

    let Opts::Contract(args) = Opts::from_args();
    match exec(args.cmd) {
        Ok(msg) => {
            println!("{}", msg);
            process::exit(0);
        }
        Err(err) => {
            eprintln!(
                "{} {}",
                "ERROR:".bright_red().bold(),
                format!("{:?}", err).bright_red()
            );
            process::exit(1);
        }
    }
}

fn exec(cmd: Command) -> Result<String> {
    match &cmd {
        Command::New { name, target_dir } => cmd::new::execute(name, target_dir.as_ref()),
        Command::Build {
            verbosity,
            unstable_options,
        } => {
            let manifest_path = Default::default();
            let dest_wasm = cmd::build::execute(
                &manifest_path,
                verbosity.clone().try_into()?,
                unstable_options.try_into()?,
            )?;
            Ok(format!(
                "\nYour contract is ready. You can find it here:\n{}",
                dest_wasm.display().to_string().bold()
            ))
        }
        Command::GenerateMetadata {
            verbosity,
            unstable_options,
        } => {
            let metadata_file = cmd::metadata::execute(
                Default::default(),
                verbosity.clone().try_into()?,
                unstable_options.try_into()?,
            )?;
            Ok(format!(
                "Your metadata file is ready.\nYou can find it here:\n{}",
                metadata_file.display()
            ))
        }
        Command::Test {} => Err(anyhow::anyhow!("Command unimplemented")),
        #[cfg(feature = "extrinsics")]
        Command::Deploy(deploy) => {
            let code_hash = deploy.exec()?;
            Ok(format!("Code hash: {}", code_hash))
        }
        #[cfg(feature = "extrinsics")]
        Command::Instantiate(instantiate) => {
            let contract_account = instantiate.run()?;
            Ok(format!("Contract account: {}", contract_account))
        }
        #[cfg(feature = "extrinsics")]
        Command::Call(call) => {
            call.run()
        }
    }
}

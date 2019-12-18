// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of ink!.
//
// ink! is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// ink! is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with ink!.  If not, see <http://www.gnu.org/licenses/>.

mod cmd;

use std::{path::PathBuf, result::Result as StdResult, str::FromStr};
use sp_core::{crypto::Pair, sr25519, H256};

use anyhow::Result;
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum AbstractionLayer {
    Core,
    Model,
    Lang,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct InvalidAbstractionLayer;

impl std::fmt::Display for InvalidAbstractionLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "expected `core`, `model` or `lang`")
    }
}

impl FromStr for AbstractionLayer {
    type Err = InvalidAbstractionLayer;

    fn from_str(input: &str) -> StdResult<Self, Self::Err> {
        match input {
            "core" => Ok(AbstractionLayer::Core),
            "model" => Ok(AbstractionLayer::Model),
            "lang" => Ok(AbstractionLayer::Lang),
            _ => Err(InvalidAbstractionLayer),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HexData(pub Vec<u8>);

impl FromStr for HexData {
    type Err = hex::FromHexError;

    fn from_str(input: &str) -> StdResult<Self, Self::Err> {
        hex::decode(input).map(HexData)
    }
}

/// Arguments required for creating and sending an extrinsic to a substrate node
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
    /// Maximum amount of gas to be used for this command
    #[structopt(name = "gas", long, default_value = "500000")]
    gas_limit: u64,
}

impl ExtrinsicOpts {
    pub fn signer(&self) -> Result<sr25519::Pair> {
        sr25519::Pair::from_string(
            &self.suri,
            self.password.as_ref().map(String::as_ref)
        ).map_err(|_| anyhow::anyhow!("Secret string error"))
    }
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Setup and create a new smart contract project
    #[structopt(name = "new")]
    New {
        /// The abstraction layer to use: `core`, `model` or `lang`
        #[structopt(short = "l", long = "layer", default_value = "lang")]
        layer: AbstractionLayer,
        /// The name of the newly created smart contract
        name: String,
        /// The optional target directory for the contract project
        #[structopt(short, long, parse(from_os_str))]
        target_dir: Option<PathBuf>,
    },
    /// Compiles the smart contract
    #[structopt(name = "build")]
    Build {},
    /// Generate contract metadata artifacts
    #[structopt(name = "generate-metadata")]
    GenerateMetadata {},
    /// Test the smart contract off-chain
    #[structopt(name = "test")]
    Test {},
    /// Upload the smart contract code to the chain
    #[cfg(feature = "deploy")]
    #[structopt(name = "deploy")]
    Deploy {
        #[structopt(flatten)]
        extrinsic_opts: ExtrinsicOpts,
        /// Path to wasm contract code, defaults to ./target/<name>-pruned.wasm
        #[structopt(parse(from_os_str))]
        wasm_path: Option<PathBuf>,
    },
    /// Instantiate a deployed smart contract
    #[cfg(feature = "deploy")]
    #[structopt(name = "instantiate")]
    Instantiate {
        #[structopt(flatten)]
        extrinsic_opts: ExtrinsicOpts,
        /// Transfers an initial balance to the instantiated contract
        #[structopt(name = "endowment", long, default_value = "0")]
        endowment: u128,
        /// The hash of the smart contract code already uploaded to the chain
        #[structopt(long, parse(try_from_str = parse_code_hash))]
        code_hash: H256,
        /// Hex encoded data to call a contract constructor
        #[structopt(long, default_value = "0x")]
        data: HexData,
    },
}

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
        Err(err) => eprintln!("error: {}", err),
    }
}

fn exec(cmd: Command) -> Result<String> {
    match &cmd {
        Command::New {
            layer,
            name,
            target_dir,
        } => cmd::execute_new(*layer, name, target_dir.as_ref()),
        Command::Build {} => cmd::execute_build(None),
        Command::GenerateMetadata {} => cmd::execute_generate_metadata(None),
        Command::Test {} => Err(anyhow::anyhow!("Command unimplemented")),
        #[cfg(feature = "deploy")]
        Command::Deploy {
            extrinsic_opts,
            wasm_path,
        } => cmd::execute_deploy(
            extrinsic_opts,
            wasm_path.as_ref(),
        ),
        #[cfg(feature = "deploy")]
        Command::Instantiate {
            extrinsic_opts,
            endowment,
            code_hash,
            data,
        } => cmd::execute_instantiate(
            extrinsic_opts,
            *endowment,
            *code_hash,
            data.clone()
        )
    }
}

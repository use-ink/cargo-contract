// Copyright 2018-2023 Parity Technologies (UK) Ltd.
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

mod config;
mod prod_chains;

pub mod build;
pub mod call;
pub mod decode;
pub mod encode;
pub mod info;
pub mod instantiate;
pub mod remove;
pub mod rpc;
pub mod schema;
pub mod storage;
pub mod upload;
pub mod verify;

pub(crate) use self::{
    build::{
        BuildCommand,
        CheckCommand,
    },
    call::CallCommand,
    decode::DecodeCommand,
    info::{
        ExtendedContractInfo,
        InfoCommand,
    },
    instantiate::InstantiateCommand,
    prod_chains::ProductionChain,
    remove::RemoveCommand,
    rpc::RpcCommand,
    schema::{
        GenerateSchemaCommand,
        VerifySchemaCommand,
    },
    storage::StorageCommand,
    upload::UploadCommand,
    verify::VerifyCommand,
};

use crate::{
    anyhow,
    PathBuf,
    Weight,
};
use anyhow::{
    Context,
    Result,
};
use colored::Colorize;
use contract_build::{
    name_value_println,
    Verbosity,
    VerbosityFlags,
    DEFAULT_KEY_COL_WIDTH,
};
pub(crate) use contract_extrinsics::ErrorVariant;
use contract_extrinsics::{
    pallet_contracts_primitives::ContractResult,
    BalanceVariant,
    TokenMetadata,
};

use std::{
    fmt::{
        Debug,
        Display,
    },
    io::{
        self,
        Write,
    },
    str::FromStr,
};

/// Arguments required for creating and sending an extrinsic to a substrate node.
#[derive(Clone, Debug, clap::Args)]
pub struct CLIExtrinsicOpts {
    /// Path to a contract build artifact file: a raw `.wasm` file, a `.contract` bundle,
    /// or a `.json` metadata file.
    #[clap(value_parser, conflicts_with = "manifest_path")]
    file: Option<PathBuf>,
    /// Path to the `Cargo.toml` of the contract.
    #[clap(long, value_parser)]
    manifest_path: Option<PathBuf>,
    /// Secret key URI for the account deploying the contract.
    ///
    /// e.g.
    /// - for a dev account "//Alice"
    /// - with a password "//Alice///SECRET_PASSWORD"
    #[clap(name = "suri", long, short)]
    suri: String,
    #[clap(flatten)]
    verbosity: VerbosityFlags,
    /// Submit the extrinsic for on-chain execution.
    #[clap(short('x'), long)]
    execute: bool,
    /// The maximum amount of balance that can be charged from the caller to pay for the
    /// storage. consumed.
    #[clap(long)]
    storage_deposit_limit: Option<String>,
    /// Before submitting a transaction, do not dry-run it via RPC first.
    #[clap(long)]
    skip_dry_run: bool,
    /// Before submitting a transaction, do not ask the user for confirmation.
    #[clap(short('y'), long)]
    skip_confirm: bool,
    /// Arguments required for communtacting with a substrate node.
    #[clap(flatten)]
    chain_cli_opts: CLIChainOpts,
}

impl CLIExtrinsicOpts {
    /// Returns the verbosity
    pub fn verbosity(&self) -> Result<Verbosity> {
        TryFrom::try_from(&self.verbosity)
    }
}

/// Arguments required for communtacting with a substrate node.
#[derive(Clone, Debug, clap::Args)]
pub struct CLIChainOpts {
    /// Websockets url of a substrate node.
    #[clap(
        name = "url",
        long,
        value_parser,
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
    /// The chain config to be used as part of the call.
    #[clap(name = "config", long, default_value = "Polkadot")]
    config: String,
    /// The name of a production chain to be communicated with.
    #[clap(name = "chain", long, conflicts_with_all = ["url", "config"])]
    chain: Option<ProductionChain>,
}

impl CLIChainOpts {
    pub fn chain(&self) -> Chain {
        if let Some(chain) = &self.chain {
            Chain::Production(chain.clone())
        } else if let Some(prod) = ProductionChain::from_parts(&self.url, &self.config) {
            Chain::Production(prod)
        } else {
            Chain::Custom(self.url.clone(), self.config.clone())
        }
    }
}

#[derive(Debug)]
pub enum Chain {
    Production(ProductionChain),
    Custom(url::Url, String),
}

impl Chain {
    pub fn url(&self) -> url::Url {
        match self {
            Chain::Production(prod) => prod.url(),
            Chain::Custom(url, _) => url.clone(),
        }
    }

    pub fn config(&self) -> &str {
        match self {
            Chain::Production(prod) => prod.config(),
            Chain::Custom(_, config) => config,
        }
    }

    pub fn production(&self) -> Option<&ProductionChain> {
        if let Chain::Production(prod) = self {
            return Some(prod)
        }
        None
    }
}

const STORAGE_DEPOSIT_KEY: &str = "Storage Total Deposit";
pub const MAX_KEY_COL_WIDTH: usize = STORAGE_DEPOSIT_KEY.len() + 1;

/// Print to stdout the fields of the result of a `instantiate` or `call` dry-run via RPC.
pub fn display_contract_exec_result<R, const WIDTH: usize, Balance>(
    result: &ContractResult<R, Balance, ()>,
) -> Result<()>
where
    Balance: Debug,
{
    let mut debug_message_lines = std::str::from_utf8(&result.debug_message)
        .context("Error decoding UTF8 debug message bytes")?
        .lines();
    name_value_println!("Gas Consumed", format!("{:?}", result.gas_consumed), WIDTH);
    name_value_println!("Gas Required", format!("{:?}", result.gas_required), WIDTH);
    name_value_println!(
        STORAGE_DEPOSIT_KEY,
        format!("{:?}", result.storage_deposit),
        WIDTH
    );

    // print debug messages aligned, only first line has key
    if let Some(debug_message) = debug_message_lines.next() {
        name_value_println!("Debug Message", format!("{debug_message}"), WIDTH);
    }

    for debug_message in debug_message_lines {
        name_value_println!("", format!("{debug_message}"), WIDTH);
    }
    Ok(())
}

pub fn display_contract_exec_result_debug<R, const WIDTH: usize, Balance>(
    result: &ContractResult<R, Balance, ()>,
) -> Result<()> {
    let mut debug_message_lines = std::str::from_utf8(&result.debug_message)
        .context("Error decoding UTF8 debug message bytes")?
        .lines();
    if let Some(debug_message) = debug_message_lines.next() {
        name_value_println!("Debug Message", format!("{debug_message}"), WIDTH);
    }

    for debug_message in debug_message_lines {
        name_value_println!("", format!("{debug_message}"), WIDTH);
    }
    Ok(())
}

pub fn display_dry_run_result_warning(command: &str) {
    println!("Your {} call {} been executed.", command, "has not".bold());
    println!(
            "To submit the transaction and execute the call on chain, add {} flag to the command.",
            "-x/--execute".bold()
        );
}

/// Prompt the user to confirm transaction submission.
pub fn prompt_confirm_tx<F: FnOnce()>(show_details: F) -> Result<()> {
    println!(
        "{} (skip with --skip-confirm or -y)",
        "Confirm transaction details:".bright_white().bold()
    );
    show_details();
    print!(
        "{} ({}/n): ",
        "Submit?".bright_white().bold(),
        "Y".bright_white().bold()
    );

    let mut buf = String::new();
    io::stdout().flush()?;
    io::stdin().read_line(&mut buf)?;
    match buf.trim().to_lowercase().as_str() {
        // default is 'y'
        "y" | "" => Ok(()),
        "n" => Err(anyhow!("Transaction not submitted")),
        c => Err(anyhow!("Expected either 'y' or 'n', got '{}'", c)),
    }
}

pub fn print_dry_running_status(msg: &str) {
    println!(
        "{:>width$} {} (skip with --skip-dry-run)",
        "Dry-running".green().bold(),
        msg.bright_white().bold(),
        width = DEFAULT_KEY_COL_WIDTH
    );
}

pub fn print_gas_required_success(gas: Weight) {
    println!(
        "{:>width$} Gas required estimated at {}",
        "Success!".green().bold(),
        gas.to_string().bright_white(),
        width = DEFAULT_KEY_COL_WIDTH
    );
}

/// Display contract information in a formatted way
pub fn basic_display_format_extended_contract_info<Hash, Balance>(
    info: &ExtendedContractInfo<Hash, Balance>,
) where
    Hash: Debug,
    Balance: Debug,
{
    name_value_println!("TrieId", info.trie_id, MAX_KEY_COL_WIDTH);
    name_value_println!(
        "Code Hash",
        format!("{:?}", info.code_hash),
        MAX_KEY_COL_WIDTH
    );
    name_value_println!(
        "Storage Items",
        format!("{:?}", info.storage_items),
        MAX_KEY_COL_WIDTH
    );
    name_value_println!(
        "Storage Items Deposit",
        format!("{:?}", info.storage_items_deposit),
        MAX_KEY_COL_WIDTH
    );
    name_value_println!(
        STORAGE_DEPOSIT_KEY,
        format!("{:?}", info.storage_total_deposit),
        MAX_KEY_COL_WIDTH
    );
    name_value_println!(
        "Source Language",
        format!("{}", info.source_language),
        MAX_KEY_COL_WIDTH
    );
}

/// Display all contracts addresses in a formatted way
pub fn display_all_contracts<AccountId>(contracts: &[AccountId])
where
    AccountId: Display,
{
    contracts.iter().for_each(|e: &AccountId| println!("{}", e))
}

/// Parse a balance from string format
pub fn parse_balance<Balance: FromStr + From<u128> + Clone>(
    balance: &str,
    token_metadata: &TokenMetadata,
) -> Result<Balance> {
    BalanceVariant::from_str(balance)
        .map_err(|e| anyhow!("Balance parsing failed: {e}"))
        .and_then(|bv| bv.denominate_balance(token_metadata))
}

/// Parse a account from string format
pub fn parse_account<AccountId: FromStr>(account: &str) -> Result<AccountId>
where
    <AccountId as FromStr>::Err: Display,
{
    AccountId::from_str(account)
        .map_err(|e| anyhow::anyhow!("Account address parsing failed: {e}"))
}

/// Parse a hex encoded 32 byte hash. Returns error if not exactly 32 bytes.
pub fn parse_code_hash<Hash>(input: &str) -> Result<Hash>
where
    Hash: From<[u8; 32]>,
{
    let bytes = contract_build::util::decode_hex(input)?;
    if bytes.len() != 32 {
        anyhow::bail!("Code hash should be 32 bytes in length")
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr.into())
}

/// Prompt the user to confirm the upload of unverifiable code to the production chain.
pub fn prompt_confirm_unverifiable_upload(chain: &str) -> Result<()> {
    println!(
        "{} (skip with --skip-validate)",
        "Confirm upload:".bright_white().bold()
    );
    let warning = format!(
        "You are trying to upload unverifiable code to {} mainnet",
        chain
    )
    .bold()
    .yellow();
    print!("{}", warning);
    println!(
        "{} ({}): ",
        "\nContinue?".bright_white().bold(),
        "y/N".bright_white().bold()
    );

    let mut buf = String::new();
    io::stdout().flush()?;
    io::stdin().read_line(&mut buf)?;
    match buf.trim().to_lowercase().as_str() {
        // default is 'n'
        "y" => Ok(()),
        "n" | "" => Err(anyhow!("Upload cancelled!")),
        c => Err(anyhow!("Expected either 'y' or 'n', got '{}'", c)),
    }
}

#[cfg(test)]
mod tests {
    use subxt::{
        Config,
        SubstrateConfig,
    };

    use super::*;

    #[test]
    fn parse_code_hash_works() {
        // with 0x prefix
        assert!(parse_code_hash::<<SubstrateConfig as Config>::Hash>(
            "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
        )
        .is_ok());
        // without 0x prefix
        assert!(parse_code_hash::<<SubstrateConfig as Config>::Hash>(
            "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
        )
        .is_ok())
    }

    #[test]
    fn parse_incorrect_len_code_hash_fails() {
        // with len not equal to 32
        assert!(parse_code_hash::<<SubstrateConfig as Config>::Hash>(
            "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da2"
        )
        .is_err())
    }

    #[test]
    fn parse_bad_format_code_hash_fails() {
        // with bad format
        assert!(parse_code_hash::<<SubstrateConfig as Config>::Hash>(
            "x43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
        )
        .is_err())
    }
}

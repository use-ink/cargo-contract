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
use super::{
    config::SignerConfig,
};
use ink_env::Environment;
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
use subxt::{
    Config,
};

/// Arguments required for creating and sending an extrinsic to a Substrate node.
#[derive(Clone, Debug, clap::Parser)]
#[clap(group = clap::ArgGroup::new("surigroup").multiple(false))]
#[clap(group = clap::ArgGroup::new("passgroup").multiple(false))]
pub struct CLIExtrinsicOpts {
    /// Path to a contract build artifact file: a raw `.wasm` file, a `.contract` bundle,
    /// or a `.json` metadata file.
    #[clap(value_parser, conflicts_with = "manifest_path")]
    file: Option<PathBuf>,
    /// Path to the `Cargo.toml` of the contract.
    #[clap(long, value_parser)]
    manifest_path: Option<PathBuf>,
    /// Secret key URI for the account interacting with the contract.
    ///
    /// e.g.
    /// - for a dev account "//Alice"
    /// - with a password "//Alice///SECRET_PASSWORD"
    /// Secret key URI for the account interacting with the contract.
    #[clap(group = "surigroup", required = false, name = "suri", long, short('s'), conflicts_with = "suri-path")]
    suri: Option<String>,
    /// Path to a file containing the secret key URI for the account interacting with the contract.
    #[clap(group = "surigroup", required = false, name = "suri-path", long, short('S'), conflicts_with = "suri")]
    suri_path: Option<PathBuf>,
    /// Password for the secret key URI.
    #[clap(group = "passgroup", required = false, name = "password", long, short('p'), conflicts_with = "password-path")]
    password: Option<String>,
    /// Path to a file containing the password for the secret key URI.
    #[clap(group = "passgroup", required = false, name = "password-path", long, short('P'), conflicts_with = "password")]
    password_path: Option<PathBuf>,
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
    /// Arguments required for communicating with a Substrate node.
    #[clap(flatten)]
    chain_cli_opts: CLIChainOpts,
}

impl CLIExtrinsicOpts {
    /// Load suri and password data.
    pub fn suri_data_from_raw(&self) -> Result<SuriData> {
        SuriData::from_suri_and_password(
            self.suri.as_ref(),
            self.password.as_ref(),
        )
    }

    /// Load suri and password data from files.
    pub fn suri_data_from_files(&self) -> Result<SuriData> {
        SuriData::from_suri_and_password_files(
            self.suri_path.as_ref(),
            self.password_path.as_ref(),
        )
    }

    // how to return change return type `<C as SignerConfig<C>>::Signer` instead of `sp_core::sr25519::Pair`
    pub fn signer<C: Config + Environment + SignerConfig<C>>(&self, extrinsic_cli_opts: CLIExtrinsicOpts) -> Result<sp_core::sr25519::Pair> {
        // initialise signer
        let mut signer = C::Signer::from_str("");
        // if suri and no password, then just load using `from_str`
        // if suri and password, then `signer_from_raw`
        // if suri_path and password_path, then load signer from `signer_from_files`
        // note: cannot provide suri and password_path, or suri_path and password.
        match &extrinsic_cli_opts.suri {
            Some(s) => {
                match &extrinsic_cli_opts.password {
                    Some(p) => {
                        // TODO - how to convert `sp_core::sr25519::Pair` into `<C as SignerConfig<C>>::Signer`?
                        signer = &extrinsic_cli_opts.signer_from_raw();
                    },
                    None => {
                        signer = match C::Signer::from_str(&s) {
                            Ok(s) => Ok(s),
                            Err(err) => Err(err),
                        };
                    },
                }
            },
            None => {
                match &extrinsic_cli_opts.suri_path {
                    Some(sp) => {
                        match &extrinsic_cli_opts.password_path {
                            Some(p) => {
                                // TODO - how to convert `sp_core::sr25519::Pair` into `<C as SignerConfig<C>>::Signer`?
                                signer = &extrinsic_cli_opts.signer_from_files();
                            },
                            None => {
                                return Err(ErrorVariant::Generic(contract_extrinsics::GenericError::from_message(format!("Failed to provide password_path required by suri_path"))));
                            },
                        }
                    },
                    None => {
                        return Err(ErrorVariant::Generic(contract_extrinsics::GenericError::from_message(format!("Failed to provide required suri or suri_path"))));
                    },
                }
            },
        };
    }

    pub fn signer_from_raw(&self) -> Result<sp_core::sr25519::Pair> {
        match &self.suri_data_from_raw() {
            // TODO - replace with `Ok` from `anyhow` throughout if possible
            // instead of using `std::result::Result`
            std::result::Result::Ok(d) => {
                // get suri
                let suri = match d.suri.suri() {
                    // remove newline characters
                    Ok(s) => s.trim().to_string(),
                    Err(e) => anyhow::bail!("suri not provided"),
                };
                // get password
                let password = match d.password.password() {
                    // remove newline characters
                    Ok(p) => Some(p.trim().to_string()),
                    Err(e) => anyhow::bail!("password not provided"),
                };
                return sp_core::Pair::from_string(&suri, password.as_ref().map(String::as_ref))
                    .map_err(|_| anyhow::anyhow!("Secret string error"))
            },
            std::result::Result::Err(_e) => anyhow::bail!("suri data not provided. {}", _e),
        };
    }

    /// Returns the signer from paths for contract extrinsics.
    pub fn signer_from_files(&self) -> Result<sp_core::sr25519::Pair> {
        match &self.suri_data_from_files() {
            // TODO - replace with `Ok` from `anyhow` throughout if possible
            // instead of using `std::result::Result`
            std::result::Result::Ok(d) => {
                // get suri
                let suri = match d.suri.suri() {
                    // remove newline characters
                    Ok(s) => s.trim().to_string(),
                    Err(e) => anyhow::bail!("suri not provided"),
                };
                // get password
                let password = match d.password.password() {
                    // remove newline characters
                    Ok(p) => Some(p.trim().to_string()),
                    Err(e) => anyhow::bail!("password not provided"),
                };
                return sp_core::Pair::from_string(&suri, password.as_ref().map(String::as_ref))
                    .map_err(|_| anyhow::anyhow!("Secret string error"))
            },
            std::result::Result::Err(_e) => anyhow::bail!("suri data not provided. {}", _e),
        };
    }

    /// Returns the verbosity
    pub fn verbosity(&self) -> Result<Verbosity> {
        TryFrom::try_from(&self.verbosity)
    }
}

/// Arguments required for communicating with a Substrate node.
#[derive(Clone, Debug, clap::Args)]
pub struct CLIChainOpts {
    /// Websockets url of a Substrate node.
    #[clap(
        name = "url",
        long,
        value_parser,
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
    /// Chain config to be used as part of the call.
    #[clap(name = "config", long, default_value = "Polkadot")]
    config: String,
    /// Name of a production chain to be communicated with.
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


/// The Suri of a contract.
#[derive(Debug)]
pub struct Suri(Option<String>);

impl Suri {
    /// The suri of the contract
    pub fn suri(&self) -> Result<&String> {
        match &self.0 {
            Some(s) => return Ok(s),
            None => anyhow::bail!("suri not available"),
        }
    }
}

/// The Password of a contract.
#[derive(Debug)]
pub struct Password(Option<String>);

impl Password {
    /// The password of the contract
    pub fn password(&self) -> Result<&String> {
        match &self.0 {
            Some(p) => return Ok(p),
            None => anyhow::bail!("password not available"),
        }
    }
}

/// Suri and password data for use with extrinsic commands.
#[derive(Debug)]
pub struct SuriData {
    /// The expected path of the file containing the suri data path.
    suri_path: Option<PathBuf>,
    /// The expected path of the file containing the password data path.
    password_path: Option<PathBuf>,
    /// The suri if data file exists.
    suri: Suri,
    /// The password if data file exists.
    password: Password,
}

impl SuriData {
    /// Given an suri and an associated password,
    /// load the metadata where possible.
    pub fn from_suri_and_password(
        suri: Option<&String>,
        password: Option<&String>,
    ) -> Result<SuriData> {
        let mut _suri = Suri(Some("".to_string()));
        let mut _password = Password(Some("".to_string()));
        match suri {
            Some(s) => {
                tracing::debug!("Reading suri");
                _suri = Suri(Some(s.to_string()));
            },
            None => {
                anyhow::bail!(
                    "Failed to load suri"
                );
            },
        }

        match password {
            Some(p) => {
                tracing::debug!("Reading password");
                _password = Password(Some(p.to_string()));
            },
            None => {
                anyhow::bail!(
                    "Failed to load password"
                );
            },
        }

        Ok(Self {
            // TODO - figure out how to avoid using `cloned()`
            suri_path: None,
            password_path: None,
            suri: _suri,
            password: _password,
        })
    }

    /// Given an suri path and an associated password path,
    /// load the metadata where possible.
    pub fn from_suri_and_password_files(
        suri_path: Option<&PathBuf>,
        password_path: Option<&PathBuf>,
    ) -> Result<SuriData> {
        let mut suri = Suri(None);
        let mut password = Password(None);

        match suri_path {
            Some(sp) => {
                match sp.extension().and_then(|ext| ext.to_str()) {
                    Some("txt") => {
                        tracing::debug!("Loading suri path from `{}`", sp.display());
                        let file_name = sp.file_stem()
                            .context("suri file has unreadable name")?
                            .to_str()
                            .context("Error parsing filename string")?;
                        // TODO - consider storing `Vec<u8>` in Suri struct instead of `String`
                        let _s: String = match String::from_utf8(std::fs::read(sp)?) {
                            std::result::Result::Ok(s) => s,
                            std::result::Result::Err(_e) => {
                                anyhow::bail!(
                                    "unable to convert Vec<u8> to String for suri_path. {}", _e
                                )
                            }
                        };
                        let s = Suri(Some(_s));
                        let dir = sp.parent().map_or_else(PathBuf::new, PathBuf::from);
                        let metadata_path = dir.join(format!("{file_name}.txt"));
                        if metadata_path.exists() {
                            suri = match s.suri() {
                                std::result::Result::Ok(s) => Suri(Some(s.to_string())),
                                std::result::Result::Err(_e) => {
                                    anyhow::bail!("unable to get value for Suri. {}", _e)
                                }
                            };
                        } else {
                            anyhow::bail!("suri file does not exist")
                        }
                        suri = match s.suri() {
                            std::result::Result::Ok(s) => Suri(Some(s.to_string())),
                            std::result::Result::Err(_e) => {
                                anyhow::bail!("unable to get value for Suri. {}", _e)
                            }
                        };
                    }
                    Some(ext) => anyhow::bail!(
                        "Invalid extension {ext}, expected `.txt`"
                    ),
                    None => {
                        anyhow::bail!(
                            "suri path has no extension, expected `.txt`"
                        )
                    }
                };
            },
            None => {
                anyhow::bail!(
                    "suri path has no extension, expected `.txt`"
                )
            }
        }

        match password_path {
            Some(pp) => {
                match pp.extension().and_then(|ext| ext.to_str()) {
                    Some("txt") => {
                        tracing::debug!("Loading password path from `{}`", pp.display());
                        let file_name = pp.file_stem()
                            .context("password file has unreadable name")?
                            .to_str()
                            .context("Error parsing filename string")?;
                        let _p: String = match String::from_utf8(std::fs::read(pp)?) {
                            std::result::Result::Ok(p) => p,
                            std::result::Result::Err(_e) => {
                                anyhow::bail!(
                                    "unable to convert Vec<u8> to String for password_path. {}", _e
                                )
                            }
                        };
                        let p = Password(Some(_p));
                        let dir = pp.parent().map_or_else(PathBuf::new, PathBuf::from);
                        let metadata_path = dir.join(format!("{file_name}.txt"));
                        if metadata_path.exists() {
                            password = match p.password() {
                                std::result::Result::Ok(p) => Password(Some(p.to_string())),
                                std::result::Result::Err(_e) => {
                                    anyhow::bail!("unable to get value for Password. {}", _e)
                                }
                            };
                        } else {
                            anyhow::bail!("password file does not exist")
                        }
                        password = match p.password() {
                            std::result::Result::Ok(p) => Password(Some(p.to_string())),
                            std::result::Result::Err(_e) => {
                                anyhow::bail!("unable to get value for Password. {}", _e)
                            }
                        };
                    }
                    Some(ext) => anyhow::bail!(
                        "Invalid extension {ext}, expected `.txt`"
                    ),
                    None => {
                        anyhow::bail!(
                            "password path has no extension, expected `.txt`"
                        )
                    }
                };
            },
            None => {
                anyhow::bail!(
                    "password path has no extension, expected `.txt`"
                )
            }
        }

        Ok(Self {
            // TODO - figure out how to avoid using `cloned()`
            suri_path: suri_path.cloned(),
            password_path: password_path.cloned(),
            suri: suri,
            password: password,
        })
    }
}

const STORAGE_DEPOSIT_KEY: &str = "Storage Total Deposit";
pub const MAX_KEY_COL_WIDTH: usize = STORAGE_DEPOSIT_KEY.len() + 1;

/// Print to stdout the fields of the result of a `instantiate` or `call` dry-run via RPC.
pub fn display_contract_exec_result<R, const WIDTH: usize, Balance>(
    result: &ContractResult<R, Balance>,
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
    result: &ContractResult<R, Balance>,
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
    println!("{}", "Confirm upload:".bright_white().bold());
    let warning = format!(
        "Warning: You are about to upload unverifiable code to {} mainnet.\n\
        A third party won't be able to confirm that your uploaded contract Wasm blob \
        matches a particular contract source code.\n\n\
        You can use `cargo contract build --verifiable` to make the contract verifiable.\n\
        See https://use.ink/basics/contract-verification for more info.",
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

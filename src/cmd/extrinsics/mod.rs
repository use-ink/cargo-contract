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

mod call;
mod events;
mod instantiate;
mod runtime_api;
mod upload;

#[cfg(test)]
#[cfg(feature = "integration-tests")]
mod integration_tests;

use anyhow::{
    anyhow,
    Context,
    Result,
};
use colored::Colorize;
use std::{
    fs::File,
    io::{
        self,
        Write,
    },
    path::PathBuf,
};

use self::events::display_events;
use crate::{
    crate_metadata::CrateMetadata,
    name_value_println,
    workspace::ManifestPath,
    Verbosity,
    VerbosityFlags,
    DEFAULT_KEY_COL_WIDTH,
};
use pallet_contracts_primitives::ContractResult;
use sp_core::{
    crypto::Pair,
    sr25519,
};
use subxt::{
    Config,
    DefaultConfig,
    HasModuleError as _,
};

pub use call::CallCommand;
pub use instantiate::InstantiateCommand;
pub use runtime_api::api::{
    DispatchError as RuntimeDispatchError,
    Event as RuntimeEvent,
};
pub use transcode::ContractMessageTranscoder;
pub use upload::UploadCommand;

type Balance = u128;
type CodeHash = <DefaultConfig as Config>::Hash;
type ContractAccount = <DefaultConfig as Config>::AccountId;
type PairSigner = subxt::PairSigner<DefaultConfig, sp_core::sr25519::Pair>;
type SignedExtra = subxt::PolkadotExtrinsicParams<DefaultConfig>;
type RuntimeApi = runtime_api::api::RuntimeApi<DefaultConfig, SignedExtra>;

/// Arguments required for creating and sending an extrinsic to a substrate node.
#[derive(Clone, Debug, clap::Args)]
pub struct ExtrinsicOpts {
    /// Path to the `Cargo.toml` of the contract.
    #[clap(long, parse(from_os_str))]
    manifest_path: Option<PathBuf>,
    /// Websockets url of a substrate node.
    #[clap(
        name = "url",
        long,
        parse(try_from_str),
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
    /// Secret key URI for the account deploying the contract.
    #[clap(name = "suri", long, short)]
    suri: String,
    /// Password for the secret key.
    #[clap(name = "password", long, short)]
    password: Option<String>,
    #[clap(flatten)]
    verbosity: VerbosityFlags,
    /// Dry-run the extrinsic via rpc, instead of as an extrinsic. Chain state will not be mutated.
    #[clap(long)]
    dry_run: bool,
    /// The maximum amount of balance that can be charged from the caller to pay for the storage
    /// consumed.
    #[clap(long, parse(try_from_str = parse_balance))]
    storage_deposit_limit: Option<Balance>,
    /// Before submitting a transaction, do not dry-run it via RPC first.
    #[clap(long)]
    skip_dry_run: bool,
    /// Before submitting a transaction, do not ask the user for confirmation.
    #[clap(long)]
    skip_confirm: bool,
}

impl ExtrinsicOpts {
    pub fn signer(&self) -> Result<sr25519::Pair> {
        sr25519::Pair::from_string(&self.suri, self.password.as_ref().map(String::as_ref))
            .map_err(|_| anyhow::anyhow!("Secret string error"))
    }

    /// Returns the verbosity
    pub fn verbosity(&self) -> Result<Verbosity> {
        TryFrom::try_from(&self.verbosity)
    }

    /// Convert URL to String without omitting the default port
    pub fn url_to_string(&self) -> String {
        let mut res = self.url.to_string();
        match (self.url.port(), self.url.port_or_known_default()) {
            (None, Some(port)) => {
                res.insert_str(res.len() - 1, &format!(":{}", port));
                res
            }
            _ => res,
        }
    }
}

/// For a contract project with its `Cargo.toml` at the specified `manifest_path`, load the cargo
/// [`CrateMetadata`] along with the contract metadata [`ink_metadata::InkProject`].
pub fn load_metadata(
    manifest_path: Option<&PathBuf>,
) -> Result<(CrateMetadata, ink_metadata::InkProject)> {
    let manifest_path = ManifestPath::try_from(manifest_path)?;
    let crate_metadata = CrateMetadata::collect(&manifest_path)?;
    let path = crate_metadata.metadata_path();

    if !path.exists() {
        return Err(anyhow!(
            "Metadata file not found. Try building with `cargo contract build`."
        ))
    }

    let file = File::open(&path)
        .context(format!("Failed to open metadata file {}", path.display()))?;
    let metadata: contract_metadata::ContractMetadata = serde_json::from_reader(file)
        .context(format!(
            "Failed to deserialize metadata file {}",
            path.display()
        ))?;
    let ink_metadata = serde_json::from_value(serde_json::Value::Object(metadata.abi))
        .context(format!(
            "Failed to deserialize ink project metadata from file {}",
            path.display()
        ))?;
    if let ink_metadata::MetadataVersioned::V3(ink_project) = ink_metadata {
        Ok((crate_metadata, ink_project))
    } else {
        Err(anyhow!("Unsupported ink metadata version. Expected V1"))
    }
}

/// Parse Rust style integer balance literals which can contain underscores.
fn parse_balance(input: &str) -> Result<Balance> {
    input
        .replace('_', "")
        .parse::<Balance>()
        .map_err(Into::into)
}

/// Create a new [`PairSigner`] from the given [`sr25519::Pair`].
pub fn pair_signer(pair: sr25519::Pair) -> PairSigner {
    PairSigner::new(pair)
}

const STORAGE_DEPOSIT_KEY: &str = "Storage Deposit";
pub const MAX_KEY_COL_WIDTH: usize = STORAGE_DEPOSIT_KEY.len() + 1;

/// Print to stdout the fields of the result of a `instantiate` or `call` dry-run via RPC.
pub fn display_contract_exec_result<R, const WIDTH: usize>(
    result: &ContractResult<R, Balance>,
) -> Result<()> {
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
        name_value_println!("Debug Message", format!("{}", debug_message), WIDTH);
    }

    for debug_message in debug_message_lines {
        name_value_println!("", format!("{}", debug_message), WIDTH);
    }
    Ok(())
}

/// Wait for the transaction to be included successfully into a block.
///
/// # Errors
///
/// If a runtime Module error occurs, this will only display the pallet and error indices. Dynamic
/// lookups of the actual error will be available once the following issue is resolved:
/// <https://github.com/paritytech/subxt/issues/443>.
///
/// # Finality
///
/// Currently this will report success once the transaction is included in a block. In the future
/// there could be a flag to wait for finality before reporting success.
async fn wait_for_success_and_handle_error<T>(
    tx_progress: subxt::TransactionProgress<'_, T, RuntimeDispatchError, RuntimeEvent>,
) -> Result<subxt::TransactionEvents<T, RuntimeEvent>>
where
    T: Config,
{
    tx_progress
        .wait_for_in_block()
        .await?
        .wait_for_success()
        .await
        .map_err(Into::into)
}

/// Extract and display error details for an RPC `--dry-run` result.
async fn dry_run_error_details(
    api: &RuntimeApi,
    error: &RuntimeDispatchError,
) -> Result<String> {
    let error = if let Some(error_data) = error.module_error_data() {
        let metadata = api.client.metadata();
        let locked_metadata = metadata.read();
        let details =
            locked_metadata.error(error_data.pallet_index, error_data.error_index())?;
        format!(
            "{}::{}: {:?}",
            details.pallet(),
            details.error(),
            details.description()
        )
    } else {
        format!("{:?}", error)
    };
    Ok(error)
}

/// Prompt the user to confirm transaction submission
fn prompt_confirm_tx<F: FnOnce()>(show_details: F) -> Result<()> {
    println!(
        "{} (skip with --skip-confirm)",
        "Confirm transaction details:".bright_white().bold()
    );
    show_details();
    print!("{} (Y/n): ", "Submit?".bright_white().bold());

    let mut buf = String::new();
    io::stdout().flush()?;
    io::stdin().read_line(&mut buf)?;
    match buf.trim() {
        "Y" => Ok(()),
        "n" => Err(anyhow!("Transaction not submitted")),
        c => Err(anyhow!("Expected either 'Y' or 'n', got '{}'", c)),
    }
}

fn print_dry_running_status(msg: &str) {
    println!(
        "{:>width$} {} (skip with --skip-dry-run)",
        "Dry-running".green().bold(),
        msg.bright_white().bold(),
        width = DEFAULT_KEY_COL_WIDTH
    );
}

fn print_gas_required_success(gas: u64) {
    println!(
        "{:>width$} Gas required estimated at {}",
        "Success!".green().bold(),
        gas.to_string().bright_white(),
        width = DEFAULT_KEY_COL_WIDTH
    );
}

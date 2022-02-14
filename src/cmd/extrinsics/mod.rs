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
mod transcode;
mod upload;

#[cfg(test)]
#[cfg(feature = "integration-tests")]
mod integration_tests;

use anyhow::{anyhow, Context, Result};
use std::{fs::File, path::PathBuf};

use self::{events::display_events, transcode::ContractMessageTranscoder};
use crate::{
    crate_metadata::CrateMetadata, name_value_println, workspace::ManifestPath, Verbosity,
    VerbosityFlags,
};
use pallet_contracts_primitives::ContractResult;
use sp_core::{crypto::Pair, sr25519};
use structopt::StructOpt;
use subxt::{Config, DefaultConfig};

pub use call::CallCommand;
pub use instantiate::InstantiateCommand;
pub use upload::UploadCommand;

type Balance = u128;
type CodeHash = <DefaultConfig as Config>::Hash;
type ContractAccount = <DefaultConfig as Config>::AccountId;
type PairSigner = subxt::PairSigner<DefaultConfig, SignedExtra, sp_core::sr25519::Pair>;
type SignedExtra = subxt::DefaultExtra<DefaultConfig>;
type RuntimeApi = runtime_api::api::RuntimeApi<DefaultConfig, SignedExtra>;

/// Arguments required for creating and sending an extrinsic to a substrate node
#[derive(Clone, Debug, StructOpt)]
pub struct ExtrinsicOpts {
    /// Path to the Cargo.toml of the contract
    #[structopt(long, parse(from_os_str))]
    manifest_path: Option<PathBuf>,
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
    /// Dry-run the extrinsic via rpc, instead of as an extrinsic. Chain state will not be mutated.
    #[structopt(long, short = "rpc")]
    dry_run: bool,
    /// The maximum amount of balance that can be charged from the caller to pay for the storage
    /// consumed.
    #[structopt(long, parse(try_from_str = parse_balance))]
    storage_deposit_limit: Option<Balance>,
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
        ));
    }

    let file =
        File::open(&path).context(format!("Failed to open metadata file {}", path.display()))?;
    let metadata: contract_metadata::ContractMetadata = serde_json::from_reader(file).context(
        format!("Failed to deserialize metadata file {}", path.display()),
    )?;
    let ink_metadata =
        serde_json::from_value(serde_json::Value::Object(metadata.abi)).context(format!(
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
pub const EXEC_RESULT_MAX_KEY_COL_WIDTH: usize = STORAGE_DEPOSIT_KEY.len() + 1;

/// Print to stdout the fields of the result of a `instantiate` or `call` dry-run via RPC.
pub fn display_contract_exec_result<R>(result: &ContractResult<R, Balance>) -> Result<()> {
    let debug_message = std::str::from_utf8(&result.debug_message)
        .context("Error decoding UTF8 debug message bytes")?;
    name_value_println!(
        "Gas Consumed",
        format!("{:?}", result.gas_consumed),
        EXEC_RESULT_MAX_KEY_COL_WIDTH
    );
    name_value_println!(
        "Gas Required",
        format!("{:?}", result.gas_required),
        EXEC_RESULT_MAX_KEY_COL_WIDTH
    );
    name_value_println!(
        STORAGE_DEPOSIT_KEY,
        format!("{:?}", result.storage_deposit),
        EXEC_RESULT_MAX_KEY_COL_WIDTH
    );
    name_value_println!(
        "Debug Message",
        format!("'{}'", debug_message),
        EXEC_RESULT_MAX_KEY_COL_WIDTH
    );
    Ok(())
}

/// Displays details of a Runtime module error.
///
/// # Note
///
/// This is currently based on the static metadata rather than the metadata fetched at runtime.
/// It means that the displayed error could be incorrect if the pallet has a different index on the
/// target chain to that in the static metadata. See
/// <https://github.com/paritytech/subxt/issues/443>
async fn wait_for_success_and_handle_error<T>(
    tx_progress: subxt::TransactionProgress<'_, T, runtime_api::api::DispatchError>,
) -> Result<subxt::TransactionEvents<T>>
where
    T: Config,
{
    tx_progress
        .wait_for_in_block()
        .await?
        .wait_for_success()
        .await
        .map_err(|e| match e {
            subxt::Error::Runtime(err) => {
                let details = err.inner().details().unwrap();
                anyhow!(
                    "Runtime: {} -> {}: {}",
                    details.pallet,
                    details.error,
                    details.docs
                )
            }
            err => err.into(),
        })
}

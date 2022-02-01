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

use anyhow::{anyhow, Context, Result};
use std::{fs::File, path::PathBuf};

use self::{events::display_events, transcode::ContractMessageTranscoder};
use crate::{crate_metadata::CrateMetadata, name_value_println, workspace::ManifestPath};
use pallet_contracts_primitives::ContractResult;
use sp_core::sr25519;
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

/// For a contract project with its `Cargo.toml` at the specified `manifest_path`, load the cargo
/// [`CrateMetadata`] along with the contract metadata [`ink_metadata::InkProject`].
pub fn load_metadata(
    manifest_path: Option<&PathBuf>,
) -> Result<(CrateMetadata, ink_metadata::InkProject)> {
    let manifest_path = ManifestPath::try_from(manifest_path)?;
    let crate_metadata = CrateMetadata::collect(&manifest_path)?;
    let path = crate_metadata.metadata_path();

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

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

use super::DefaultConfig;
use anyhow::{
    anyhow,
    Result,
};
use contract_extrinsics::{
    ContractArtifacts,
    ContractStorageKey,
    ContractStorageLayout,
    ContractStorageRpc,
    ErrorVariant,
};
use std::{
    fmt::Debug,
    path::PathBuf,
};
use subxt::Config;

#[derive(Debug, clap::Args)]
#[clap(name = "storage", about = "Inspect contract storage")]
pub struct StorageCommand {
    /// The address of the contract to inspect storage of.
    #[clap(
        name = "contract",
        long,
        env = "CONTRACT",
        required_unless_present = "all"
    )]
    contract: <DefaultConfig as Config>::AccountId,
    /// Path to a contract build artifact file: a raw `.wasm` file, a `.contract` bundle,
    /// or a `.json` metadata file.
    #[clap(value_parser, conflicts_with = "manifest_path")]
    file: Option<PathBuf>,
    /// Path to the `Cargo.toml` of the contract.
    #[clap(long, value_parser)]
    manifest_path: Option<PathBuf>,
    /// Websockets url of a substrate node.
    #[clap(
        name = "url",
        long,
        value_parser,
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
}

impl StorageCommand {
    pub async fn run(&self) -> Result<(), ErrorVariant> {
        let rpc = ContractStorageRpc::new(&self.url).await?;

        let contract_artifacts = ContractArtifacts::from_manifest_or_file(
            self.manifest_path.as_ref(),
            self.file.as_ref(),
        )?;

        let contract_info = rpc.fetch_contract_info(&self.contract).await?;

        let trie_id = contract_info.trie_id();
        let storage_layout =
            ContractStorageLayout::try_from(contract_artifacts.metadata())?;
        let root_key = storage_layout.root_key();

        // todo: fetch all storage keys and map to metadata?
        let storage_keys = rpc
            .fetch_storage_keys_paged(trie_id, None, 100, None, None)
            .await?;

        for storage_key in storage_keys {
            println!("storage key: {}", hex::encode(storage_key));
        }

        let root_storage = rpc
            .fetch_contract_storage(trie_id, &root_key, None)
            .await?
            .ok_or(anyhow!(
                "No contract storage was found for account id {}",
                self.contract
            ))?;

        let root_cell = ContractStorageCell {
            key: root_key.hashed_to_hex(),
            value: hex::encode(root_storage),
        };

        let contract_storage = ContractStorage { root: root_cell };

        println!(
            "{json}",
            json = serde_json::to_string_pretty(&contract_storage)?
        );

        Ok(())
    }
}

#[derive(serde::Serialize)]
struct ContractStorage {
    root: ContractStorageCell,
}

#[derive(serde::Serialize)]

struct ContractStorageCell {
    key: String,
    value: String,
}

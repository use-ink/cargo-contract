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
use anyhow::Result;
use colored::Colorize;
use contract_extrinsics::{
    ContractArtifacts,
    ContractStorage,
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
    #[clap(name = "contract", long, env = "CONTRACT")]
    contract: <DefaultConfig as Config>::AccountId,
    /// Fetch the "raw" storage keys and values for the contract.
    #[clap(long)]
    raw: bool,
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
        let rpc = ContractStorageRpc::<DefaultConfig>::new(&self.url).await?;
        let storage_layout = ContractStorage::<DefaultConfig>::new(rpc);

        if self.raw {
            let storage_data = storage_layout
                .load_contract_storage_data(&self.contract)
                .await?;
            println!(
                "{json}",
                json = serde_json::to_string_pretty(&storage_data)?
            );
            return Ok(())
        }

        let contract_artifacts = ContractArtifacts::from_manifest_or_file(
            self.manifest_path.as_ref(),
            self.file.as_ref(),
        );

        match contract_artifacts {
            Ok(contract_artifacts) => {
                let ink_metadata = contract_artifacts.ink_project_metadata()?;
                let contract_storage = storage_layout
                    .load_contract_storage_with_layout(&ink_metadata, &self.contract)
                    .await?;
                println!(
                    "{json}",
                    json = serde_json::to_string_pretty(&contract_storage)?
                );
            }
            Err(_) => {
                eprintln!(
                    "{} Displaying raw storage: no valid contract metadata artifacts found",
                    "Info:".cyan().bold(),
                );
                let storage_data = storage_layout
                    .load_contract_storage_data(&self.contract)
                    .await?;
                println!(
                    "{json}",
                    json = serde_json::to_string_pretty(&storage_data)?
                );
                return Ok(())
            }
        }

        Ok(())
    }
}

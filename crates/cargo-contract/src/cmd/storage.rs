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
    ContractStorageCell,
    ContractStorageLayout,
    ContractStorageRpc,
    ErrorVariant,
};
use contract_transcode::ContractMessageTranscoder;
use sp_core::hexdisplay::AsBytesRef;
use std::path::PathBuf;
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
    /// Export the instantiate output in JSON format.
    #[clap(name = "output-json", long, conflicts_with = "raw")]
    output_json: bool,
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
                if self.output_json {
                    println!(
                        "{json}",
                        json = serde_json::to_string_pretty(&contract_storage)?
                    );
                } else {
                    let transcoder = contract_artifacts.contract_transcoder()?;
                    Self::display_storage_table(&contract_storage, &transcoder)?;
                }
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

    fn display_storage_table(
        storage: &ContractStorageLayout,
        transcoder: &ContractMessageTranscoder,
    ) -> Result<()> {
        let storage = DisplayStorageLayout::new(storage, transcoder);

        println!(
            "{:<10} | {:<20.20} | {}",
            "Root Key".bright_purple().bold(),
            "Parent".bright_purple().bold(),
            "Value".bright_purple().bold()
        );

        for cell in storage.iter() {
            for value in &cell.value {
                println!("{:<10} | {:<20.20} | {}", cell.root_key, cell.parent, value);
            }
        }
        Ok(())
    }
}

struct DisplayStorageItem {
    root_key: String,
    parent: String,
    value: Vec<String>,
}

struct DisplayStorageLayout<'a> {
    storage: &'a ContractStorageLayout,
    transcoder: &'a ContractMessageTranscoder,
}

impl<'a> DisplayStorageLayout<'a> {
    pub fn new(
        storage: &'a ContractStorageLayout,
        transcoder: &'a ContractMessageTranscoder,
    ) -> Self {
        Self {
            storage,
            transcoder,
        }
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter {
            inner: self.storage.cells.iter().peekable(),
            transcoder: self.transcoder,
        }
    }
}

struct Iter<'a> {
    inner: std::iter::Peekable<std::slice::Iter<'a, ContractStorageCell>>,
    transcoder: &'a ContractMessageTranscoder,
}

impl<'a> Iter<'a> {
    fn storage_mapping_value(
        &mut self,
        map_first_item: &ContractStorageCell,
    ) -> Option<Vec<String>> {
        let mut item = map_first_item;
        let type_def = self
            .transcoder
            .metadata()
            .registry()
            .resolve(item.type_id)?;
        let key_type_id = type_def.type_params.iter().find(|&e| e.name == "K")?.ty?.id;
        let value_type_id = type_def.type_params.iter().find(|&e| e.name == "V")?.ty?.id;
        let mut values = Vec::new();
        loop {
            let mapping_key = item.mapping_key.clone()?;

            let key = self
                .transcoder
                .decode(key_type_id, &mut mapping_key.as_bytes_ref())
                .ok()?;

            let value = self
                .transcoder
                .decode(value_type_id, &mut item.value.as_bytes_ref())
                .ok()?;

            values.push(format!("Mapping {{ {} => {} }}", key, value));
            if self.inner.peek().map(|e| e.root_key == item.root_key) != Some(true) {
                break
            } else {
                item = self.inner.next()?;
            }
        }
        Some(values)
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = DisplayStorageItem;

    fn next(&mut self) -> Option<DisplayStorageItem> {
        if let Some(item) = self.inner.next() {
            let type_def = self
                .transcoder
                .metadata()
                .registry()
                .resolve(item.type_id)?;

            let value = match type_def.path.to_string().as_str() {
                "ink_storage::lazy::mapping::Mapping" => {
                    self.storage_mapping_value(item)?
                }
                "ink_storage::lazy::Lazy" => {
                    let value_type_id =
                        type_def.type_params.iter().find(|&e| e.name == "V")?.ty?.id;
                    vec![self
                        .transcoder
                        .decode(value_type_id, &mut item.value.as_bytes_ref())
                        .map(|e| e.to_string())
                        .ok()?]
                }
                _ => {
                    vec![self
                        .transcoder
                        .decode(item.type_id, &mut item.value.as_bytes_ref())
                        .map(|e| e.to_string())
                        .ok()?]
                }
            };
            let parent = item.path.last().cloned()?;

            Some(DisplayStorageItem {
                root_key: hex::encode(item.root_key.to_le_bytes()),
                parent,
                value,
            })
        } else {
            None
        }
    }
}

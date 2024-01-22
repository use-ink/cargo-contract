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
use colored::Colorize;
use contract_extrinsics::{
    ContractArtifacts,
    ContractStorage,
    ContractStorageLayout,
    ContractStorageRpc,
    ErrorVariant,
};
use crossterm::terminal;
use ink_env::DefaultEnvironment;
use std::{
    cmp,
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
        let rpc = ContractStorageRpc::<DefaultConfig, DefaultEnvironment>::new(&self.url)
            .await?;
        let storage_layout =
            ContractStorage::<DefaultConfig, DefaultEnvironment>::new(rpc);

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
                let transcoder = contract_artifacts.contract_transcoder()?;
                let contract_storage = storage_layout
                    .load_contract_storage_with_layout(&self.contract, &transcoder)
                    .await?;
                if self.output_json {
                    println!(
                        "{json}",
                        json = serde_json::to_string_pretty(&contract_storage)?
                    );
                } else {
                    let table = StorageDisplayTable::new(&contract_storage)?;
                    table.display()?;
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
}

struct StorageDisplayTable<'a> {
    storage_layout: &'a ContractStorageLayout,
    parent_width: usize,
    value_width: usize,
}

impl<'a> StorageDisplayTable<'a> {
    const KEY_WIDTH: usize = 8;
    const INDEX_WIDTH: usize = 5;
    const INDEX_LABEL: &'static str = "Index";
    const KEY_LABEL: &'static str = "Root Key";
    const PARENT_LABEL: &'static str = "Parent";
    const VALUE_LABEL: &'static str = "Value";

    fn new(storage_layout: &'a ContractStorageLayout) -> Result<Self> {
        let parent_len = storage_layout
            .iter()
            .map(|c| c.parent().len())
            .max()
            .unwrap_or_default();
        let parent_width = cmp::max(parent_len, Self::PARENT_LABEL.len());
        let terminal_width = terminal::size().unwrap_or((80, 24)).0 as usize;

        // There are tree separators in the table ' | '
        let value_width = terminal_width
            .checked_sub(Self::KEY_WIDTH + Self::INDEX_WIDTH + 3 * 3 + parent_width)
            .filter(|&w| w > Self::VALUE_LABEL.len())
            .ok_or(anyhow!(
                "Terminal width to small to display the storage layout correctly"
            ))?;

        Ok(Self {
            storage_layout,
            parent_width,
            value_width,
        })
    }

    fn table_row_println(&self, index: usize, key: &str, parent: &str, value: &str) {
        let mut result = value.split_whitespace().fold(
            (Vec::new(), String::new()),
            |(mut result, mut current_line), word| {
                if current_line.len() + word.len() + 1 > self.value_width {
                    if !current_line.is_empty() {
                        result.push(current_line.clone());
                        current_line.clear();
                    }
                    current_line.push_str(word);
                    (result, current_line)
                } else {
                    if !current_line.is_empty() {
                        current_line.push(' ');
                    }
                    current_line.push_str(word);
                    (result, current_line)
                }
            },
        );

        if !result.1.is_empty() {
            result.0.push(result.1);
        }

        for (i, value) in result.0.iter().enumerate() {
            println!(
                "{:<index_width$} | {:<key_width$} | {:<parent_width$} | {:<value_width$.value_width$}",
                if i == 0 { index.to_string() } else { String::new() },
                if i == 0 { key } else { "" },
                if i == 0 { parent } else { "" },
                value,
                index_width = Self::INDEX_WIDTH,
                key_width = Self::KEY_WIDTH,
                parent_width = self.parent_width,
                value_width = self.value_width,
            );
        }
    }

    fn display(&self) -> Result<()> {
        // Print the table header
        println!(
            "{:<index_width$} | {:<key_width$} | {:<parent_width$} | {:<value_width$.value_width$}",
            Self::INDEX_LABEL.bright_purple().bold(),
            Self::KEY_LABEL.bright_purple().bold(),
            Self::PARENT_LABEL.bright_purple().bold(),
            Self::VALUE_LABEL.bright_purple().bold(),
            index_width = Self::INDEX_WIDTH,
            key_width = Self::KEY_WIDTH,
            parent_width = self.parent_width,
            value_width = self.value_width,
        );

        for (index, cell) in self.storage_layout.iter().enumerate() {
            let formatted_cell = format!("{cell}");
            let values = formatted_cell.split('\n');
            for (i, v) in values.enumerate() {
                self.table_row_println(
                    index + i,
                    cell.root_key().as_str(),
                    cell.parent().as_str(),
                    v,
                );
            }
        }
        Ok(())
    }
}

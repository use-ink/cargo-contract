// Copyright (C) Use Ink (UK) Ltd.
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

use super::{
    parse_account,
    CLIChainOpts,
};
use crate::call_with_config;
use anyhow::Result;
use colored::Colorize;
use comfy_table::{
    ContentArrangement,
    Table,
};
use contract_extrinsics::{
    ContractArtifacts,
    ContractStorage,
    ContractStorageLayout,
    ContractStorageRpc,
    ErrorVariant,
};
use ink_env::Environment;
use serde::Serialize;
use std::{
    fmt::Display,
    path::PathBuf,
    str::FromStr,
};
use subxt::{
    config::HashFor,
    ext::{
        codec::Decode,
        scale_decode::IntoVisitor,
    },
    Config,
};

#[derive(Debug, clap::Args)]
#[clap(name = "storage", about = "Inspect contract storage")]
pub struct StorageCommand {
    /// The address of the contract to inspect storage of.
    #[clap(
        name = "contract",
        long,
        env = "CONTRACT",
        required_unless_present = "version"
    )]
    contract: Option<String>,
    /// Fetch the "raw" storage keys and values for the contract.
    #[clap(long)]
    raw: bool,
    /// Export the instantiate output in JSON format.
    #[clap(name = "output-json", long, conflicts_with = "raw")]
    output_json: bool,
    /// Path to a contract build artifact file: a raw `.polkavm` file, a `.contract`
    /// bundle, or a `.json` metadata file.
    #[clap(value_parser, conflicts_with = "manifest_path")]
    file: Option<PathBuf>,
    /// Path to the `Cargo.toml` of the contract.
    #[clap(long, value_parser)]
    manifest_path: Option<PathBuf>,
    /// Fetch the storage version of the pallet contracts (state query:
    /// contracts::palletVersion()).
    #[clap(long, short)]
    version: bool,
    /// Arguments required for communicating with a Substrate node.
    #[clap(flatten)]
    chain_cli_opts: CLIChainOpts,
}

impl StorageCommand {
    pub async fn handle(&self) -> Result<(), ErrorVariant> {
        call_with_config!(self, run, self.chain_cli_opts.chain().config())
    }

    pub async fn run<C: Config + Environment>(&self) -> Result<(), ErrorVariant>
    where
        <C as Config>::AccountId: Display + IntoVisitor + AsRef<[u8]> + FromStr + Decode,
        <<C as Config>::AccountId as FromStr>::Err:
            Into<Box<dyn std::error::Error>> + Display,
        C::Balance: Serialize + IntoVisitor,
        HashFor<C>: IntoVisitor,
    {
        let rpc =
            ContractStorageRpc::<C>::new(&self.chain_cli_opts.chain().url()).await?;
        let storage_layout = ContractStorage::<C, C>::new(rpc);
        if self.version {
            println!("{}", storage_layout.version().await?);
            return Ok(())
        }

        // Contract arg shall be always present in this case, it is enforced by
        // clap configuration
        let contract = self
            .contract
            .as_ref()
            .map(|c| parse_account(c))
            .transpose()?
            .expect("Contract argument shall be present");

        if self.raw {
            let storage_data =
                storage_layout.load_contract_storage_data(&contract).await?;
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
                    .load_contract_storage_with_layout(&contract, &transcoder)
                    .await?;
                if self.output_json {
                    println!(
                        "{json}",
                        json = serde_json::to_string_pretty(&contract_storage)?
                    );
                } else {
                    let table = StorageDisplayTable::new(&contract_storage);
                    table.display();
                }
            }
            Err(_) => {
                eprintln!(
                    "{} Displaying raw storage: no valid contract metadata artifacts found",
                    "Info:".cyan().bold(),
                );
                let storage_data =
                    storage_layout.load_contract_storage_data(&contract).await?;
                println!(
                    "{json}",
                    json = serde_json::to_string_pretty(&storage_data)?
                );
            }
        }

        Ok(())
    }
}

struct StorageDisplayTable(Table);

impl StorageDisplayTable {
    const INDEX_LABEL: &'static str = "Index";
    const KEY_LABEL: &'static str = "Root Key";
    const PARENT_LABEL: &'static str = "Parent";
    const VALUE_LABEL: &'static str = "Value";

    fn new(storage_layout: &ContractStorageLayout) -> Self {
        let mut table = Table::new();
        Self::table_add_header(&mut table);
        Self::table_add_rows(&mut table, storage_layout);
        Self(table)
    }

    fn table_add_header(table: &mut Table) {
        table.set_content_arrangement(ContentArrangement::Dynamic);

        let header = vec![
            Self::INDEX_LABEL,
            Self::KEY_LABEL,
            Self::PARENT_LABEL,
            Self::VALUE_LABEL,
        ];
        table.set_header(header);
    }

    fn table_add_rows(table: &mut Table, storage_layout: &ContractStorageLayout) {
        for (index, cell) in storage_layout.iter().enumerate() {
            let formatted_cell = format!("{cell}");
            let values = formatted_cell.split('\n');
            for (i, v) in values.enumerate() {
                table.add_row(vec![
                    (index + i).to_string().as_str(),
                    cell.root_key().as_str(),
                    cell.parent().as_str(),
                    v,
                ]);
            }
        }
    }

    fn display(&self) {
        println!("{}", self.0);
    }
}

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

use super::{
    runtime_api::api::{self,},
    Client,
    DefaultConfig,
};
use crate::{
    cmd::{
        runtime_api::api::runtime_types::pallet_contracts::storage::ContractInfo,
        Balance,
        CodeHash,
        ErrorVariant,
    },
    name_value_println,
};
use anyhow::{
    anyhow,
    Result,
};
use std::fmt::Debug;
use subxt::{
    Config,
    OnlineClient,
};

#[derive(Debug, clap::Args)]
#[clap(name = "info", about = "Get infos from a contract")]
pub struct InfoCommand {
    /// The address of the contract to display info of.
    #[clap(name = "contract", long, env = "CONTRACT")]
    contract: <DefaultConfig as Config>::AccountId,
    /// Websockets url of a substrate node.
    #[clap(
        name = "url",
        long,
        value_parser,
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
    /// Export the instantiate output in JSON format.
    #[clap(name = "output-json", long)]
    output_json: bool,
}

impl InfoCommand {
    pub fn run(&self) -> Result<(), ErrorVariant> {
        tracing::debug!(
            "Getting contract information for AccountId {:?}",
            self.contract
        );

        async_std::task::block_on(async {
            let url = self.url.clone();
            let client = OnlineClient::<DefaultConfig>::from_url(url).await?;

            let info_result = self.fetch_contract_info(&client).await?;

            match info_result {
                Some(info_result) => {
                    let convert_trie_id = hex::encode(info_result.trie_id.0);
                    let info_to_json = InfoToJson {
                        trie_id: convert_trie_id,
                        code_hash: info_result.code_hash,
                        storage_items: info_result.storage_items,
                        storage_item_deposit: info_result.storage_item_deposit,
                    };
                    if self.output_json {
                        println!("{}", info_to_json.to_json()?);
                    } else {
                        info_to_json.basic_display_format_contract_info();
                    }
                    Ok(())
                }
                None => {
                    Err(anyhow!(
                        "No contract information was found for account id {}",
                        self.contract
                    )
                    .into())
                }
            }
        })
    }

    async fn fetch_contract_info(&self, client: &Client) -> Result<Option<ContractInfo>> {
        let info_contract_call =
            api::storage().contracts().contract_info_of(&self.contract);

        let contract_info_of = client
            .storage()
            .at(None)
            .await?
            .fetch(&info_contract_call)
            .await?;

        Ok(contract_info_of)
    }
}

#[derive(serde::Serialize)]
struct InfoToJson {
    trie_id: String,
    code_hash: CodeHash,
    storage_items: u32,
    storage_item_deposit: Balance,
}

impl InfoToJson {
    /// Convert and return contract info in JSON format.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Display contract information in a formatted way
    pub fn basic_display_format_contract_info(&self) {
        name_value_println!("TrieId:", format!("{}", self.trie_id));
        name_value_println!("Code hash:", format!("{:?}", self.code_hash));
        name_value_println!("Storage items:", format!("{:?}", self.storage_items));
        name_value_println!(
            "Storage deposit:",
            format!("{:?}", self.storage_item_deposit)
        );
    }
}

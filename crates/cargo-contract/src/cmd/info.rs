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
        ErrorVariant,
    },
    name_value_println,
    DEFAULT_KEY_COL_WIDTH,
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
    /// The address of the the contract to call.
    #[clap(name = "contract", long, env = "CONTRACT")]
    contract: <DefaultConfig as Config>::AccountId,
    #[clap(
        name = "url",
        long,
        value_parser,
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
    /// Export the call output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

impl InfoCommand {
    pub fn is_json(&self) -> bool {
        self.output_json
    }

    pub fn run(&self) -> Result<(), ErrorVariant> {
        tracing::debug!(
            "Getting information for contract AccountId {:?}",
            self.contract
        );

        async_std::task::block_on(async {
            let url = self.url.clone();
            let client = OnlineClient::<DefaultConfig>::from_url(url).await?;

            let info_result = self.fetch_contract_info(&client).await?;

            match info_result {
                Some(info_result) => {
                    InfoCommand::print_and_format_contract_info(info_result);
                }
                None => {
                    return Err(anyhow!(
                        "No contract information was found for the ContractId {}",
                        self.contract
                    )
                    .into())
                }
            }
            Result::<(), ErrorVariant>::Ok(())
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

    fn print_and_format_contract_info(info: ContractInfo) {
        name_value_println!(
            "TrieId:",
            format!("{:?}", info.trie_id),
            DEFAULT_KEY_COL_WIDTH
        );
        name_value_println!(
            "Code hash:",
            format!("{:?}", info.code_hash),
            DEFAULT_KEY_COL_WIDTH
        );
        name_value_println!(
            "Number of storage items:",
            format!("{:?}", info.storage_items),
            DEFAULT_KEY_COL_WIDTH
        );
        name_value_println!(
            "Storage deposit:",
            format!("{:?}", info.storage_item_deposit),
            DEFAULT_KEY_COL_WIDTH
        );
    }
}

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
    runtime_api::api::{self},
    Balance, Client, CodeHash, DefaultConfig, ExtrinsicOpts,
};
use crate::cmd::extrinsics::runtime_api::api::runtime_types::pallet_contracts::storage::ContractInfo;
use crate::{cmd::extrinsics::ErrorVariant, name_value_println, DEFAULT_KEY_COL_WIDTH};
use anyhow::{anyhow, Result};
use scale::{Decode, Encode};
use sp_weights::Weight;
use std::fmt::Debug;
use subxt::{Config, OnlineClient};

#[derive(Debug, clap::Args)]
#[clap(name = "info", about = "Get infos from a contract")]
pub struct InfoCommand {
    /// The address of the the contract to call.
    #[clap(name = "contract", long, env = "CONTRACT")]
    contract: <DefaultConfig as Config>::AccountId,
    #[clap(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Export the call output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

impl InfoCommand {
    pub fn is_json(&self) -> bool {
        self.output_json
    }

    pub fn run(&self) -> Result<(), ErrorVariant> {
        if let _account_id = Some(self.contract.clone()) {
            async_std::task::block_on(async {
                let url = self.extrinsic_opts.url_to_string();
                let client = OnlineClient::<DefaultConfig>::from_url(url.clone()).await?;

                tracing::debug!(
                    "Getting information for contract AccountId {:?}",
                    self.contract
                );

                let info_result = self.info_dry_run(&client).await?;

                match info_result {
                    Some(info_result) => {
                        println!("{:?}", info_result);
                    }
                    None => {
                        return Err(anyhow!(
                            "No contract information were found for the contract Id {}",
                            self.contract
                        )
                        .into());
                    }
                }
                Result::<(), ErrorVariant>::Ok(())
            })
        } else {
            return Err(anyhow!("Please provide an accountId with --contract").into());
        }
    }

    async fn info_dry_run(&self, client: &Client) -> Result<Option<ContractInfo>> {
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

/// A struct that encodes RPC parameters required for a call to a smart contract.
///
/// Copied from `pallet-contracts-rpc-runtime-api`.
#[derive(Encode)]
pub struct InfoRequest {
    origin: <DefaultConfig as Config>::AccountId,
    dest: <DefaultConfig as Config>::AccountId,
    value: Balance,
    gas_limit: Option<Weight>,
    storage_deposit_limit: Option<Balance>,
}

/// Result of the contract info
#[derive(Debug, Decode, Clone, PartialEq, Eq, serde::Serialize)]
pub struct InfoDryResult {
    /// Result of a dry run
    // pub trie_id: BoundedVec<u8, ConstU32<128>>,
    pub code_hash: CodeHash,
    pub storage_bytes: u32,
    pub storage_items: u32,
    pub storage_byte_deposit: Balance,
    pub storage_item_deposit: Balance,
    // pub storage_base_deposit: StorageDeposit
}

impl InfoDryResult {
    /// Returns a result in json format
    // pub fn to_json(&self) -> Result<String> {
    //     Ok(serde_json::to_string_pretty(self)?)
    // }

    pub fn print(&self) {
        name_value_println!(
            "Result code_hash",
            format!("{:?}", self.code_hash),
            DEFAULT_KEY_COL_WIDTH
        );
        // name_value_println!(
        //     "Result storage_items",
        //     format!("{:?}", self.storage_items),
        //     DEFAULT_KEY_COL_WIDTH
        // );
        // name_value_println!(
        //     "Result storage_item_deposit {:?}",
        //     format!("{:?}", self.storage_item_deposit)
        // );
        // name_value_println!(
        //     "Result storage_items",
        //     format!("{:?}", self.storage_items),
        //     DEFAULT_KEY_COL_WIDTH
        // );
    }
}

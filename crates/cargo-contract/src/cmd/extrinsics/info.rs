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
    runtime_api::api::{
        self
    },
    Balance,
    CodeHash,
    DefaultConfig,
    ExtrinsicOpts
};
use crate::{
    cmd::extrinsics::ErrorVariant,
    name_value_println,
    DEFAULT_KEY_COL_WIDTH
};
use anyhow::Result;
use scale::Encode;
use sp_weights::Weight;
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
    
        let artifacts = self.extrinsic_opts.contract_artifacts()?;        
        let signer = super::pair_signer(self.extrinsic_opts.signer()?);

        async_std::task::block_on(async {
            let url = self.extrinsic_opts.url_to_string();
            let client = OnlineClient::<DefaultConfig>::from_url(url.clone()).await?;

            if self.extrinsic_opts.dry_run {
                let info_result = self.info_rpc().await;
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Error when trying to get info for contract AccountId {}",
                    self.contract
                )
                .into())
            }
        })
    }

    async fn info_rpc(
        &self
    ) -> () {

        tracing::debug!("Getting information for contract AccountId {:?}", self.contract);
        let info_contract_call = api::storage().contracts().contract_info_of(
            self.contract,
        );
        //info_contract_call;
        println!("{:?}", info_contract_call);
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
#[derive(serde::Serialize)]
pub struct InfoDryResult {
    /// Result of a dry run 
    pub trie_id: String,
    /// Was the operation reverted
    pub code_hash: CodeHash,
    pub storage_bytes: u32,
    pub storage_items: u32,
    pub storage_byte_deposit: Balance,
    /// This records to how much deposit the accumulated `storage_items` amount to
    pub storage_item_deposit: Balance,
    pub storage_base_deposit: Balance
}

impl InfoDryResult {
    /// Returns a result in json format
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn print(&self) {
        name_value_println!("Result storage_bytes", self.storage_bytes);
        name_value_println!(
            "Result storage_items",
            format!("{:?}", self.storage_items),
            DEFAULT_KEY_COL_WIDTH
        );
    }
}
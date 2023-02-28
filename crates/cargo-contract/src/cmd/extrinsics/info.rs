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
        self,
        runtime_types::pallet_contracts::wasm::Determinism,
    },
    state_call,
    submit_extrinsic,
    Balance,
    Client,
    CodeHash,
    DefaultConfig,
    ExtrinsicOpts,
    PairSigner,
    TokenMetadata
};
use crate::{
    cmd::extrinsics::{
        events::DisplayEvents,
        ErrorVariant,
        WasmCode,
    },
    name_value_println,
};
use anyhow::Result;
use scale::Encode;
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
            let client = OnlineClient::from_url(url.clone()).await?;

            if self.extrinsic_opts.dry_run {
                let result = self.info_dry_run(&client, &signer).await?;

                match result.result {
                    Ok(ref ret_val) => {
                        let value = transcoder
                            .decode_return(&self.message, &mut &ret_val.data[..])
                            .context(format!(
                                "Failed to decode return value {:?}",
                                &ret_val
                            ))?;
                        let dry_run_result = InfoDryResult {
                            trie_id: '',
                            reverted: result.code_hash,
                            storage_bytes: result.storage_bytes,
                            storage_items: result.storage_items,
                            storage_byte_deposit: result.storage_byte_deposit,
                            storage_item_deposit: result.storage_item_deposit,
                            storage_base_deposit: Balance::from(
                                &result.storage_deposit,
                            )
                        };
                        if self.output_json {
                            println!("{}", dry_run_result.to_json()?);
                        } else {
                            dry_run_result.print();
                            display_contract_exec_result_debug::<_, DEFAULT_KEY_COL_WIDTH>(
                                &result,
                            )?;
                        }
                        Ok(())
                    }
                    Err(ref err) => {
                        let metadata = client.metadata();
                        let object = ErrorVariant::from_dispatch_error(err, &metadata)?;
                        if self.output_json {
                            Err(object)
                        } else {
                            name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                            display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(
                                &result,
                            )?;
                            Ok(())
                        }
                    }
                }
            } else {
                println!("Error when trying to get contract info");
            }
        })
    }

    async fn info(
        &self,
        code: Vec<u8>,
        gas_limit: Weight,
    ) -> Result<(), ErrorVariant> {

        let info_contract_call = api::tx().contracts().instantiate_with_code(
            self.args.value,
            gas_limit,
            self.args.storage_deposit_limit_compact(),
            code.to_vec(),
            self.args.data.clone(),
            self.args.salt.clone(),
        );

        let contract_info = info_contract_call.client()
    }

    // to change to get the same kind of format https://github.com/paritytech/subxt/blob/master/testing/integration-tests/src/frame/contracts.rs#L214-L219
    async fn info_dry_run(
        &self,
        client: &Client,
        signer: &PairSigner,
    ) -> Result<ContractExecResult<Balance>> {
        let url = self.extrinsic_opts.url_to_string();
        let token_metadata = TokenMetadata::query(client).await?;
        let storage_deposit_limit = self
            .extrinsic_opts
            .storage_deposit_limit
            .as_ref()
            .map(|bv| bv.denominate_balance(&token_metadata))
            .transpose()?;
        let info_request = InfoRequest {
            origin: signer.account_id().clone(),
            dest: self.contract.clone(),
            value: self.value.denominate_balance(&token_metadata)?,
            gas_limit: None,
            storage_deposit_limit
        };
        state_call(&url, "ContractsApi_call", call_request).await
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
    // /// Result of a dry run 
    // pub trie_id: String,
    // /// Was the operation reverted
    // pub code_hash: CodeHash,
    // pub storage_bytes: u32,
    // pub storage_items: u32,
    // pub storage_byte_deposit: Balance,
    // /// This records to how much deposit the accumulated `storage_items` amount to
    // pub storage_item_deposit: Balance,
    // pub storage_base_deposit: Balance
}

impl InfoDryResult {
    /// Returns a result in json format
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn print(&self) {
        name_value_println!("Result", self.result, DEFAULT_KEY_COL_WIDTH);
        name_value_println!(
            "Reverted",
            format!("{:?}", self.reverted),
            DEFAULT_KEY_COL_WIDTH
        );
        name_value_println!("Data", format!("{}", self.data), DEFAULT_KEY_COL_WIDTH);
    }
}
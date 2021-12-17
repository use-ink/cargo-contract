// Copyright 2018-2020 Parity Technologies (UK) Ltd.
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
    display_events, load_metadata, pretty_print, Balance, ContractMessageTranscoder, PairSigner,
    RuntimeApi,
};
use crate::ExtrinsicOpts;
use anyhow::Result;
use colored::Colorize;
use jsonrpsee::{
    types::{to_json_value, traits::Client as _},
    ws_client::WsClientBuilder,
};
use serde::Serialize;
use sp_core::Bytes;
use std::fmt::Debug;
use structopt::StructOpt;
use subxt::{rpc::NumberOrHex, ClientBuilder, Config, DefaultConfig, Signer};

type ContractExecResult = pallet_contracts_primitives::ContractExecResult<Balance>;

#[derive(Debug, StructOpt)]
#[structopt(name = "call", about = "Call a contract")]
pub struct CallCommand {
    /// The address of the the contract to call.
    #[structopt(name = "contract", long, env = "CONTRACT")]
    contract: <DefaultConfig as Config>::AccountId,
    /// The name of the contract message to call.
    #[structopt(long, short)]
    message: String,
    /// The arguments of the contract message to call.
    #[structopt(long)]
    args: Vec<String>,
    #[structopt(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Maximum amount of gas to be used for this command.
    #[structopt(name = "gas", long, default_value = "50000000000")]
    gas_limit: u64,
    /// The maximum amount of balance that can be charged from the caller to pay for the storage
    /// consumed.
    #[structopt(long)]
    storage_deposit_limit: Option<Balance>,
    /// The value to be transferred as part of the call.
    #[structopt(name = "value", long, default_value = "0")]
    value: Balance,
    /// Dry-run the call via rpc, instead of as an extrinsic. Contract state will not be mutated.
    #[structopt(long, short = "rpc")]
    dry_run: bool,
}

impl CallCommand {
    pub fn run(&self) -> Result<()> {
        let metadata = load_metadata()?;
        let transcoder = ContractMessageTranscoder::new(&metadata);
        let call_data = transcoder.encode(&self.message, &self.args)?;
        let signer = super::pair_signer(self.extrinsic_opts.signer()?);

        async_std::task::block_on(async {
            if self.dry_run {
                self.call_rpc(call_data, &signer, &transcoder).await
            } else {
                self.call(call_data, &signer, &transcoder).await
            }
        })
    }

    async fn call_rpc<'a>(
        &self,
        data: Vec<u8>,
        signer: &PairSigner,
        transcoder: &ContractMessageTranscoder<'a>,
    ) -> Result<()> {
        let url = self.extrinsic_opts.url.to_string();
        let cli = WsClientBuilder::default().build(&url).await?;
        let storage_deposit_limit = self
            .storage_deposit_limit
            .as_ref()
            .map(|limit| NumberOrHex::Hex((*limit).into()));
        let call_request = RpcCallRequest {
            origin: signer.account_id().clone(),
            dest: self.contract.clone(),
            value: NumberOrHex::Hex(self.value.into()),
            gas_limit: NumberOrHex::Number(self.gas_limit),
            storage_deposit_limit,
            input_data: Bytes(data),
        };
        let params = vec![to_json_value(call_request)?];
        let result: ContractExecResult = cli.request("contracts_call", Some(params.into())).await?;

        let exec_return_value = result
            .result
            .map_err(|e| anyhow::anyhow!("Failed to execute call via rpc: {:?}", e))?;
        let value = transcoder.decode_return(&self.message, exec_return_value.data.0)?;
        pretty_print(value, false)?;
        println!("{:?} {}", "Gas consumed:".bold(), result.gas_consumed);
        Ok(())
    }

    async fn call<'a>(
        &self,
        data: Vec<u8>,
        signer: &PairSigner,
        transcoder: &ContractMessageTranscoder<'a>,
    ) -> Result<()> {
        let url = self.extrinsic_opts.url.to_string();
        let api = ClientBuilder::new()
            .set_url(&url)
            .build()
            .await?
            .to_runtime_api::<RuntimeApi>();

        log::debug!("calling contract {:?}", self.contract);
        let result = api
            .tx()
            .contracts()
            .call(
                self.contract.clone().into(),
                self.value,
                self.gas_limit,
                self.storage_deposit_limit,
                data,
            )
            .sign_and_submit_then_watch(signer)
            .await?
            .wait_for_finalized_success()
            .await?;

        display_events(
            &result,
            &transcoder,
            api.client.metadata(),
            &self.extrinsic_opts.verbosity()?,
        )
    }
}

/// A struct that encodes RPC parameters required for a call to a smart-contract.
///
/// Copied from pallet-contracts-rpc
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcCallRequest {
    origin: <DefaultConfig as Config>::AccountId,
    dest: <DefaultConfig as Config>::AccountId,
    value: NumberOrHex,
    gas_limit: NumberOrHex,
    storage_deposit_limit: Option<NumberOrHex>,
    input_data: Bytes,
}

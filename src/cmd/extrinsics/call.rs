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
    display_contract_exec_result,
    display_events,
    load_metadata,
    parse_balance,
    wait_for_success_and_handle_error,
    Balance,
    Client,
    ContractMessageTranscoder,
    ContractsRpcError,
    DefaultConfig,
    ExtrinsicOpts,
    PairSigner,
    EXEC_RESULT_MAX_KEY_COL_WIDTH,
};
use crate::name_value_println;
use anyhow::Result;
use jsonrpsee::{
    core::client::ClientT,
    rpc_params,
    ws_client::WsClientBuilder,
};
use pallet_contracts_primitives::{
    ContractResult,
    ExecReturnValue,
};
use serde::Serialize;
use sp_core::Bytes;
use std::{
    fmt::Debug,
    result,
};
use subxt::{
    rpc::NumberOrHex,
    Config,
    OnlineClient,
};

type ContractExecResult =
    ContractResult<result::Result<ExecReturnValue, ContractsRpcError>, Balance>;

#[derive(Debug, clap::Args)]
#[clap(name = "call", about = "Call a contract")]
pub struct CallCommand {
    /// The address of the the contract to call.
    #[clap(name = "contract", long, env = "CONTRACT")]
    contract: <DefaultConfig as Config>::AccountId,
    /// The name of the contract message to call.
    #[clap(long, short)]
    message: String,
    /// The arguments of the contract message to call.
    #[clap(long, multiple_values = true)]
    args: Vec<String>,
    #[clap(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Maximum amount of gas to be used for this command.
    #[clap(name = "gas", long, default_value = "50000000000")]
    gas_limit: u64,
    /// The value to be transferred as part of the call.
    #[clap(name = "value", long, parse(try_from_str = parse_balance), default_value = "0")]
    value: Balance,
}

impl CallCommand {
    pub fn run(&self) -> Result<()> {
        let (_, contract_metadata) =
            load_metadata(self.extrinsic_opts.manifest_path.as_ref())?;
        let transcoder = ContractMessageTranscoder::new(&contract_metadata);
        let call_data = transcoder.encode(&self.message, &self.args)?;
        log::debug!("Message data: {:?}", hex::encode(&call_data));

        let signer = super::pair_signer(self.extrinsic_opts.signer()?);

        async_std::task::block_on(async {
            let url = self.extrinsic_opts.url_to_string();
            let client = OnlineClient::from_url(url.clone()).await?;

            if self.extrinsic_opts.dry_run {
                self.call_rpc(&client, call_data, &signer, &transcoder)
                    .await
            } else {
                self.call(&client, call_data, &signer, &transcoder).await
            }
        })
    }

    async fn call_rpc(
        &self,
        client: &Client,
        data: Vec<u8>,
        signer: &PairSigner,
        transcoder: &ContractMessageTranscoder<'_>,
    ) -> Result<()> {
        let url = self.extrinsic_opts.url_to_string();
        let ws_client = WsClientBuilder::default().build(&url).await?;
        let storage_deposit_limit = self
            .extrinsic_opts
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
        let params = rpc_params![call_request];
        let result: ContractExecResult =
            ws_client.request("contracts_call", params).await?;

        match result.result {
            Ok(ref ret_val) => {
                let value =
                    transcoder.decode_return(&self.message, &mut &ret_val.data.0[..])?;
                name_value_println!(
                    "Result",
                    String::from("Success!"),
                    EXEC_RESULT_MAX_KEY_COL_WIDTH
                );
                name_value_println!(
                    "Reverted",
                    format!("{:?}", ret_val.did_revert()),
                    EXEC_RESULT_MAX_KEY_COL_WIDTH
                );
                name_value_println!(
                    "Data",
                    format!("{}", value),
                    EXEC_RESULT_MAX_KEY_COL_WIDTH
                );
            }
            Err(ref err) => {
                let metadata = client.metadata();
                let err = err.error_details(&metadata)?;
                name_value_println!("Result", err, EXEC_RESULT_MAX_KEY_COL_WIDTH);
            }
        }
        display_contract_exec_result(&result)?;
        Ok(())
    }

    async fn call(
        &self,
        client: &Client,
        data: Vec<u8>,
        signer: &PairSigner,
        transcoder: &ContractMessageTranscoder<'_>,
    ) -> Result<()> {
        log::debug!("calling contract {:?}", self.contract);

        let call = super::runtime_api::api::tx().contracts().call(
            self.contract.clone().into(),
            self.value,
            self.gas_limit,
            self.extrinsic_opts.storage_deposit_limit,
            data,
        );

        let tx_progress = client
            .tx()
            .sign_and_submit_then_watch_default(&call, signer)
            .await?;

        let result = wait_for_success_and_handle_error(tx_progress).await?;

        display_events(
            &result,
            transcoder,
            &client.metadata(),
            &self.extrinsic_opts.verbosity()?,
        )
    }
}

/// A struct that encodes RPC parameters required for a call to a smart contract.
///
/// Copied from `pallet-contracts-rpc`.
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

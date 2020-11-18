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

use super::{display_events, load_metadata, pretty_print, Transcoder};
use crate::ExtrinsicOpts;
use anyhow::Result;
use colored::Colorize;
use jsonrpsee::common::Params;
use serde::{Deserialize, Serialize};
use sp_core::Bytes;
use sp_rpc::number::NumberOrHex;
use std::{convert::TryInto, fmt::Debug};
use structopt::StructOpt;
use subxt::{
    balances::Balances, contracts::*, system::*, ClientBuilder, ContractsTemplateRuntime,
    ExtrinsicSuccess, Signer,
};

#[derive(Debug, StructOpt)]
#[structopt(name = "call", about = "Call a contract")]
pub struct CallCommand {
    /// The name of the contract message to call.
    name: String,
    /// The arguments of the contract message to call.
    args: Vec<String>,
    #[structopt(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Maximum amount of gas to be used for this command.
    #[structopt(name = "gas", long, default_value = "50000000000")]
    gas_limit: u64,
    /// The value to be transferred as part of the call.
    #[structopt(name = "value", long, default_value = "0")]
    value: <ContractsTemplateRuntime as Balances>::Balance,
    /// The address of the the contract to call.
    #[structopt(name = "contract", long, env = "CONTRACT")]
    contract: <ContractsTemplateRuntime as System>::AccountId,
    /// Perform the call via rpc, instead of as an extrinsic. Contract state will not be mutated.
    #[structopt(name = "rpc", long)]
    rpc: bool,
}

impl CallCommand {
    pub fn run(&self) -> Result<String> {
        let metadata = load_metadata()?;
        let transcoder = Transcoder::new(&metadata);
        let call_data = transcoder.encode(&self.name, &self.args)?;

        if self.rpc {
            let result = async_std::task::block_on(self.call_rpc(call_data))?;
            match result {
                RpcContractExecResult::Success {
                    data, gas_consumed, ..
                } => {
                    let value = transcoder.decode_return(&self.name, data.0)?;
                    pretty_print(value, false)?;
                    Ok(format!("{} {}", "Gas consumed:".bold(), gas_consumed))
                }
                RpcContractExecResult::Error(()) => {
                    Err(anyhow::anyhow!("Failed to execute call via rpc"))
                }
            }
        } else {
            let result = async_std::task::block_on(self.call(call_data))?;
            display_events(&result, &transcoder, self.extrinsic_opts.verbosity()?);
            Ok("".into())
        }
    }

    async fn call_rpc(&self, data: Vec<u8>) -> Result<RpcContractExecResult> {
        let url = self.extrinsic_opts.url.to_string();
        let cli = jsonrpsee::ws_client(&url).await?;
        let signer = self.extrinsic_opts.signer()?;
        let call_request = RpcCallRequest {
            origin: signer.account_id().clone(),
            dest: self.contract.clone(),
            value: self.value.try_into()?, // value must be <= u64.max_value() for now
            gas_limit: NumberOrHex::Number(self.gas_limit),
            input_data: Bytes(data),
        };
        let params = Params::Array(vec![serde_json::to_value(call_request)?]);
        let result: RpcContractExecResult = cli.request("contracts_call", params).await?;
        Ok(result)
    }

    async fn call(&self, data: Vec<u8>) -> Result<ExtrinsicSuccess<ContractsTemplateRuntime>> {
        let cli = ClientBuilder::<ContractsTemplateRuntime>::new()
            .set_url(&self.extrinsic_opts.url.to_string())
            .build()
            .await?;
        let signer = self.extrinsic_opts.signer()?;

        let extrinsic_success = cli
            .call_and_watch(&signer, &self.contract, self.value, self.gas_limit, &data)
            .await?;
        Ok(extrinsic_success)
    }
}

/// Call request type for serialization copied from pallet-contracts-rpc
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcCallRequest {
    origin: <ContractsTemplateRuntime as System>::AccountId,
    dest: <ContractsTemplateRuntime as System>::AccountId,
    // Should be <ContractsTemplateRuntime as Balances>::Balance, which is u128.
    // However the max unsigned integer supported by serde is `u64`
    value: u64,
    gas_limit: NumberOrHex,
    input_data: Bytes,
}

/// Result of contract execution copied from pallet-contracts-rpc
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub enum RpcContractExecResult {
    /// Successful execution
    Success {
        /// The return flags
        flags: u32,
        /// Output data
        data: Bytes,
        /// How much gas was consumed by the call.
        gas_consumed: u64,
    },
    /// Error execution
    Error(()),
}

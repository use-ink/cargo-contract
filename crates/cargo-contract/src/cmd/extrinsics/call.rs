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
    prompt_confirm_tx,
    runtime_api::api,
    state_call,
    submit_extrinsic,
    Balance,
    BalanceVariant,
    Client,
    ContractMessageTranscoder,
    DefaultConfig,
    ExtrinsicOpts,
    PairSigner,
    StorageDeposit,
    TokenMetadata,
    MAX_KEY_COL_WIDTH,
};

use crate::{
    cmd::extrinsics::{
        display_contract_exec_result_debug,
        events::DisplayEvents,
        ErrorVariant,
    },
    DEFAULT_KEY_COL_WIDTH,
};
use contract_build::name_value_println;

use anyhow::{
    anyhow,
    Context,
    Result,
};

use contract_transcode::Value;
use pallet_contracts_primitives::ContractExecResult;
use scale::Encode;
use sp_weights::Weight;

use std::fmt::Debug;
use subxt::{
    Config,
    OnlineClient,
};

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
    #[clap(long, num_args = 0..)]
    args: Vec<String>,
    #[clap(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Maximum amount of gas (execution time) to be used for this command.
    /// If not specified will perform a dry-run to estimate the gas consumed for the call.
    #[clap(name = "gas", long)]
    gas_limit: Option<u64>,
    /// Maximum proof size for this call.
    /// If not specified will perform a dry-run to estimate the proof size required for the call.
    #[clap(long)]
    proof_size: Option<u64>,
    /// The value to be transferred as part of the call.
    #[clap(name = "value", long, default_value = "0")]
    value: BalanceVariant,
    /// Export the call output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

impl CallCommand {
    pub fn is_json(&self) -> bool {
        self.output_json
    }

    pub fn run(&self) -> Result<(), ErrorVariant> {
        let artifacts = self.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;

        let call_data = transcoder.encode(&self.message, &self.args)?;
        tracing::debug!("Message data: {:?}", hex::encode(&call_data));

        let signer = super::pair_signer(self.extrinsic_opts.signer()?);

        async_std::task::block_on(async {
            let url = self.extrinsic_opts.url_to_string();
            let client = OnlineClient::from_url(url.clone()).await?;

            let result = self
                .call_dry_run(call_data.clone(), &client, &signer)
                .await?;

            match result.result {
                Ok(ref ret_val) => {
                    let value = transcoder
                        .decode_return(&self.message, &mut &ret_val.data[..])
                        .context(format!(
                            "Failed to decode return value {:?}",
                            &ret_val
                        ))?;
                    let dry_run_result = CallDryRunResult {
                        result: String::from("Success!"),
                        reverted: ret_val.did_revert(),
                        data: value,
                        gas_consumed: result.gas_consumed,
                        gas_required: result.gas_required,
                        storage_deposit: StorageDeposit::from(&result.storage_deposit),
                    };
                    if self.output_json {
                        println!("{}", dry_run_result.to_json()?);
                    } else {
                        dry_run_result.print();
                        display_contract_exec_result_debug::<_, DEFAULT_KEY_COL_WIDTH>(
                            &result,
                        )?;
                    }
                }
                Err(ref err) => {
                    let metadata = client.metadata();
                    let object = ErrorVariant::from_dispatch_error(err, &metadata)?;
                    if self.output_json {
                        return Err(object)
                    } else {
                        name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                        display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(&result)?;
                    }
                }
            }
            if self.extrinsic_opts.execute {
                self.call(&client, call_data, &signer, &transcoder).await?;
            }
            Ok(())
        })
    }

    async fn call_dry_run(
        &self,
        input_data: Vec<u8>,
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
        let call_request = CallRequest {
            origin: signer.account_id().clone(),
            dest: self.contract.clone(),
            value: self.value.denominate_balance(&token_metadata)?,
            gas_limit: None,
            storage_deposit_limit,
            input_data,
        };
        state_call(&url, "ContractsApi_call", call_request).await
    }

    async fn call(
        &self,
        client: &Client,
        data: Vec<u8>,
        signer: &PairSigner,
        transcoder: &ContractMessageTranscoder,
    ) -> Result<(), ErrorVariant> {
        tracing::debug!("calling contract {:?}", self.contract);

        let gas_limit = self
            .pre_submit_dry_run_gas_estimate(client, data.clone(), signer)
            .await?;

        if !self.extrinsic_opts.skip_confirm {
            prompt_confirm_tx(|| {
                name_value_println!("Message", self.message, DEFAULT_KEY_COL_WIDTH);
                name_value_println!("Args", self.args.join(" "), DEFAULT_KEY_COL_WIDTH);
                name_value_println!(
                    "Gas limit",
                    gas_limit.to_string(),
                    DEFAULT_KEY_COL_WIDTH
                );
            })?;
        }

        let token_metadata = TokenMetadata::query(client).await?;

        let call = api::tx().contracts().call(
            self.contract.clone().into(),
            self.value.denominate_balance(&token_metadata)?,
            gas_limit,
            self.extrinsic_opts.storage_deposit_limit(&token_metadata)?,
            data,
        );

        let result = submit_extrinsic(client, &call, signer).await?;

        let display_events =
            DisplayEvents::from_events(&result, Some(transcoder), &client.metadata())?;

        let output = if self.output_json {
            display_events.to_json()?
        } else {
            display_events
                .display_events(self.extrinsic_opts.verbosity()?, &token_metadata)?
        };
        println!("{output}");

        Ok(())
    }

    /// Dry run the call before tx submission. Returns the gas required estimate.
    async fn pre_submit_dry_run_gas_estimate(
        &self,
        client: &Client,
        data: Vec<u8>,
        signer: &PairSigner,
    ) -> Result<Weight> {
        if self.extrinsic_opts.skip_dry_run {
            return match (self.gas_limit, self.proof_size) {
                (Some(ref_time), Some(proof_size)) => Ok(Weight::from_parts(ref_time, proof_size)),
                _ => {
                    Err(anyhow!(
                    "Weight args `--gas` and `--proof-size` required if `--skip-dry-run` specified"
                ))
                }
            }
        }
        if !self.output_json {
            super::print_dry_running_status(&self.message);
        }
        let call_result = self.call_dry_run(data, client, signer).await?;
        match call_result.result {
            Ok(_) => {
                if !self.output_json {
                    super::print_gas_required_success(call_result.gas_required);
                }
                // use user specified values where provided, otherwise use the estimates
                let ref_time = self
                    .gas_limit
                    .unwrap_or_else(|| call_result.gas_required.ref_time());
                let proof_size = self
                    .proof_size
                    .unwrap_or_else(|| call_result.gas_required.proof_size());
                Ok(Weight::from_parts(ref_time, proof_size))
            }
            Err(ref err) => {
                let object = ErrorVariant::from_dispatch_error(err, &client.metadata())?;
                if self.output_json {
                    Err(anyhow!("{}", serde_json::to_string_pretty(&object)?))
                } else {
                    name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                    display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(&call_result)?;
                    Err(anyhow!("Pre-submission dry-run failed. Use --skip-dry-run to skip this step."))
                }
            }
        }
    }
}

/// A struct that encodes RPC parameters required for a call to a smart contract.
///
/// Copied from `pallet-contracts-rpc-runtime-api`.
#[derive(Encode)]
pub struct CallRequest {
    origin: <DefaultConfig as Config>::AccountId,
    dest: <DefaultConfig as Config>::AccountId,
    value: Balance,
    gas_limit: Option<Weight>,
    storage_deposit_limit: Option<Balance>,
    input_data: Vec<u8>,
}

/// Result of the contract call
#[derive(serde::Serialize)]
pub struct CallDryRunResult {
    /// Result of a dry run
    pub result: String,
    /// Was the operation reverted
    pub reverted: bool,
    pub data: Value,
    pub gas_consumed: Weight,
    pub gas_required: Weight,
    /// Storage deposit after the operation
    pub storage_deposit: StorageDeposit,
}

impl CallDryRunResult {
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

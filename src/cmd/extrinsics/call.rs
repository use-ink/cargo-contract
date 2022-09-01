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
    error_details,
    parse_balance,
    prompt_confirm_tx,
    state_call,
    submit_extrinsic,
    Balance,
    Client,
    ContractMessageTranscoder,
    CrateMetadata,
    DefaultConfig,
    ExtrinsicOpts,
    PairSigner,
    MAX_KEY_COL_WIDTH,
};
use crate::{
    name_value_println,
    DEFAULT_KEY_COL_WIDTH,
};
use anyhow::{
    anyhow,
    Result,
};

use pallet_contracts_primitives::ContractExecResult;
use scale::Encode;

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
    #[clap(long, multiple_values = true)]
    args: Vec<String>,
    #[clap(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Maximum amount of gas to be used for this command.
    /// If not specified will perform a dry-run to estimate the gas consumed for the instantiation.
    #[clap(name = "gas", long)]
    gas_limit: Option<u64>,
    /// The value to be transferred as part of the call.
    #[clap(name = "value", long, parse(try_from_str = parse_balance), default_value = "0")]
    value: Balance,
}

impl CallCommand {
    pub fn run(&self) -> Result<()> {
        let crate_metadata = CrateMetadata::from_manifest_path(
            self.extrinsic_opts.manifest_path.as_ref(),
        )?;
        let transcoder = ContractMessageTranscoder::load(crate_metadata.metadata_path())?;
        let call_data = transcoder.encode(&self.message, &self.args)?;
        tracing::debug!("Message data: {:?}", hex::encode(&call_data));

        let signer = super::pair_signer(self.extrinsic_opts.signer()?);

        async_std::task::block_on(async {
            let url = self.extrinsic_opts.url_to_string();
            let client = OnlineClient::from_url(url.clone()).await?;

            if self.extrinsic_opts.dry_run {
                let result = self.call_dry_run(call_data, &signer).await?;

                match result.result {
                    Ok(ref ret_val) => {
                        let value = transcoder
                            .decode_return(&self.message, &mut &ret_val.data.0[..])?;
                        name_value_println!(
                            "Result",
                            String::from("Success!"),
                            DEFAULT_KEY_COL_WIDTH
                        );
                        name_value_println!(
                            "Reverted",
                            format!("{:?}", ret_val.did_revert()),
                            DEFAULT_KEY_COL_WIDTH
                        );
                        name_value_println!(
                            "Data",
                            format!("{}", value),
                            DEFAULT_KEY_COL_WIDTH
                        );
                        display_contract_exec_result::<_, DEFAULT_KEY_COL_WIDTH>(&result)
                    }
                    Err(ref err) => {
                        let err = error_details(err, &client.metadata())?;
                        name_value_println!("Result", err, MAX_KEY_COL_WIDTH);
                        display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(&result)
                    }
                }
            } else {
                self.call(&client, call_data, &signer, &transcoder).await
            }
        })
    }

    async fn call_dry_run(
        &self,
        input_data: Vec<u8>,
        signer: &PairSigner,
    ) -> Result<ContractExecResult<Balance>> {
        let url = self.extrinsic_opts.url_to_string();
        let gas_limit = *self.gas_limit.as_ref().unwrap_or(&5_000_000_000_000);
        let storage_deposit_limit = self.extrinsic_opts.storage_deposit_limit;
        let call_request = CallRequest {
            origin: signer.account_id().clone(),
            dest: self.contract.clone(),
            value: self.value,
            gas_limit,
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
    ) -> Result<()> {
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

        let call = super::runtime_api::api::tx().contracts().call(
            self.contract.clone().into(),
            self.value,
            gas_limit,
            self.extrinsic_opts.storage_deposit_limit,
            data,
        );

        let result = submit_extrinsic(client, &call, signer).await?;

        display_events(
            &result,
            transcoder,
            &client.metadata(),
            &self.extrinsic_opts.verbosity()?,
        )
    }

    /// Dry run the call before tx submission. Returns the gas required estimate.
    async fn pre_submit_dry_run_gas_estimate(
        &self,
        client: &Client,
        data: Vec<u8>,
        signer: &PairSigner,
    ) -> Result<u64> {
        if self.extrinsic_opts.skip_dry_run {
            return match self.gas_limit {
                Some(gas) => Ok(gas),
                None => {
                    Err(anyhow!(
                    "Gas limit `--gas` argument required if `--skip-dry-run` specified"
                ))
                }
            }
        }
        super::print_dry_running_status(&self.message);
        let call_result = self.call_dry_run(data, signer).await?;
        match call_result.result {
            Ok(_) => {
                super::print_gas_required_success(call_result.gas_required);
                let gas_limit = self.gas_limit.unwrap_or(call_result.gas_required);
                Ok(gas_limit)
            }
            Err(ref err) => {
                let err = error_details(err, &client.metadata())?;
                name_value_println!("Result", err, MAX_KEY_COL_WIDTH);
                display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(&call_result)?;
                Err(anyhow!("Pre-submission dry-run failed. Use --skip-dry-run to skip this step."))
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
    gas_limit: u64,
    storage_deposit_limit: Option<Balance>,
    input_data: Vec<u8>,
}

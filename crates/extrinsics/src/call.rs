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
    account_id,
    display_contract_exec_result,
    events::DisplayEvents,
    runtime_api::api,
    state,
    state_call,
    submit_extrinsic,
    AccountId32,
    Balance,
    BalanceVariant,
    Client,
    ContractMessageTranscoder,
    DefaultConfig,
    ErrorVariant,
    ExtrinsicOpts,
    Missing,
    StorageDeposit,
    TokenMetadata,
    DEFAULT_KEY_COL_WIDTH,
    MAX_KEY_COL_WIDTH,
};

use anyhow::{
    anyhow,
    Result,
};
use contract_build::name_value_println;
use contract_transcode::Value;
use pallet_contracts_primitives::ContractExecResult;
use scale::Encode;
use sp_weights::Weight;
use subxt_signer::sr25519::Keypair;

use core::marker::PhantomData;
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
    /// If not specified will perform a dry-run to estimate the gas consumed for the
    /// call.
    #[clap(name = "gas", long)]
    gas_limit: Option<u64>,
    /// Maximum proof size for this call.
    /// If not specified will perform a dry-run to estimate the proof size required for
    /// the call.
    #[clap(long)]
    proof_size: Option<u64>,
    /// The value to be transferred as part of the call.
    #[clap(name = "value", long, default_value = "0")]
    value: BalanceVariant,
    /// Export the call output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

/// A builder for the call command.
pub struct CallCommandBuilder<Message, ExtrinsicOptions> {
    opts: CallCommand,
    marker: PhantomData<fn() -> (Message, ExtrinsicOptions)>,
}

impl<E> CallCommandBuilder<Missing<state::Message>, E> {
    /// Sets the name of the contract message to call.
    pub fn message<T: Into<String>>(
        self,
        message: T,
    ) -> CallCommandBuilder<state::Message, E> {
        CallCommandBuilder {
            opts: CallCommand {
                message: message.into(),
                ..self.opts
            },
            marker: PhantomData,
        }
    }
}

impl<M> CallCommandBuilder<M, Missing<state::ExtrinsicOptions>> {
    /// Sets the extrinsic operation.
    pub fn extrinsic_opts(
        self,
        extrinsic_opts: ExtrinsicOpts,
    ) -> CallCommandBuilder<M, state::ExtrinsicOptions> {
        CallCommandBuilder {
            opts: CallCommand {
                extrinsic_opts,
                ..self.opts
            },
            marker: PhantomData,
        }
    }
}

impl<M, E> CallCommandBuilder<M, E> {
    /// Sets the the address of the the contract to call.
    pub fn contract(self, contract: <DefaultConfig as Config>::AccountId) -> Self {
        let mut this = self;
        this.opts.contract = contract;
        this
    }

    /// Sets the arguments of the contract message to call.
    pub fn args(self, args: Vec<String>) -> Self {
        let mut this = self;
        this.opts.args = args;
        this
    }

    /// Sets the maximum amount of gas to be used for this command.
    pub fn gas_limit(self, gas_limit: u64) -> Self {
        let mut this = self;
        this.opts.gas_limit = Some(gas_limit);
        this
    }

    /// Sets the maximum proof size for this call.
    pub fn proof_size(self, proof_size: u64) -> Self {
        let mut this = self;
        this.opts.proof_size = Some(proof_size);
        this
    }

    /// Sets the value to be transferred as part of the call.
    pub fn value(self, value: BalanceVariant) -> Self {
        let mut this = self;
        this.opts.value = value;
        this
    }

    /// Sets whether to export the call output in JSON format.
    pub fn output_json(self, output_json: bool) -> Self {
        let mut this = self;
        this.opts.output_json = output_json;
        this
    }
}

impl CallCommandBuilder<state::Message, state::ExtrinsicOptions> {
    /// Finishes construction of the call command.
    pub async fn done(self) -> CallExec {
        let call_command = self.opts;
        call_command.preprocess().await.unwrap()
    }
}

#[allow(clippy::new_ret_no_self)]
impl CallCommand {
    /// Returns a clean builder for [`CallCommand`].
    pub fn new(
    ) -> CallCommandBuilder<Missing<state::Message>, Missing<state::ExtrinsicOptions>>
    {
        CallCommandBuilder {
            opts: Self {
                contract: AccountId32([0; 32]),
                message: String::new(),
                args: Vec::new(),
                extrinsic_opts: ExtrinsicOpts::default(),
                gas_limit: None,
                proof_size: None,
                value: "0".parse().unwrap(),
                output_json: false,
            },
            marker: PhantomData,
        }
    }

    pub fn is_json(&self) -> bool {
        self.output_json
    }

    /// Helper method for preprocessing contract artifacts.
    pub async fn preprocess(&self) -> Result<CallExec> {
        let artifacts = self.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;

        let call_data = transcoder.encode(&self.message, &self.args)?;
        tracing::debug!("Message data: {:?}", hex::encode(&call_data));

        let signer = self.extrinsic_opts.signer()?;

        let url = self.extrinsic_opts.url_to_string();
        let client = OnlineClient::from_url(url.clone()).await?;
        Ok(CallExec {
            contract: self.contract.clone(),
            message: self.message.clone(),
            args: self.args.clone(),
            opts: self.extrinsic_opts.clone(),
            gas_limit: self.gas_limit,
            proof_size: self.proof_size,
            value: self.value.clone(),
            output_json: self.output_json,
            client,
            transcoder,
            call_data,
            signer,
        })
    }
}

pub struct CallExec {
    contract: <DefaultConfig as Config>::AccountId,
    message: String,
    args: Vec<String>,
    opts: ExtrinsicOpts,
    gas_limit: Option<u64>,
    proof_size: Option<u64>,
    value: BalanceVariant,
    output_json: bool,
    client: Client,
    transcoder: ContractMessageTranscoder,
    call_data: Vec<u8>,
    signer: Keypair,
}

impl CallExec {
    pub async fn call_dry_run(&self) -> Result<ContractExecResult<Balance, ()>> {
        let url = self.opts.url_to_string();
        let token_metadata = TokenMetadata::query(&self.client).await?;
        let storage_deposit_limit = self
            .opts
            .storage_deposit_limit
            .as_ref()
            .map(|bv| bv.denominate_balance(&token_metadata))
            .transpose()?;
        let call_request = CallRequest {
            origin: account_id(&self.signer),
            dest: self.contract.clone(),
            value: self.value.denominate_balance(&token_metadata)?,
            gas_limit: None,
            storage_deposit_limit,
            input_data: self.call_data.clone(),
        };
        state_call(&url, "ContractsApi_call", call_request).await
    }

    pub async fn call(&self, gas_limit: Weight) -> Result<DisplayEvents, ErrorVariant> {
        tracing::debug!("calling contract {:?}", self.contract);
        let token_metadata = TokenMetadata::query(&self.client).await?;

        let call = api::tx().contracts().call(
            self.contract.clone().into(),
            self.value.denominate_balance(&token_metadata)?,
            gas_limit.into(),
            self.opts.storage_deposit_limit(&token_metadata)?,
            self.call_data.clone(),
        );

        let result = submit_extrinsic(&self.client, &call, &self.signer).await?;

        let display_events = DisplayEvents::from_events(
            &result,
            Some(&self.transcoder),
            &self.client.metadata(),
        )?;

        Ok(display_events)
    }

    /// Dry run the call before tx submission. Returns the gas required estimate.
    pub async fn pre_submit_dry_run_gas_estimate(
        &self,
        print_to_terminal: bool,
    ) -> Result<Weight> {
        if self.opts.skip_dry_run {
            return match (self.gas_limit, self.proof_size) {
                (Some(ref_time), Some(proof_size)) => Ok(Weight::from_parts(ref_time, proof_size)),
                _ => {
                    Err(anyhow!(
                    "Weight args `--gas` and `--proof-size` required if `--skip-dry-run` specified"
                ))
                }
            };
        }
        if !self.output_json && print_to_terminal {
            super::print_dry_running_status(&self.message);
        }
        let call_result = self.call_dry_run().await?;
        match call_result.result {
            Ok(_) => {
                if !self.output_json && print_to_terminal {
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
                let object =
                    ErrorVariant::from_dispatch_error(err, &self.client.metadata())?;
                if self.output_json {
                    Err(anyhow!("{}", serde_json::to_string_pretty(&object)?))
                } else {
                    if print_to_terminal {
                        name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                        display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(
                            &call_result,
                        )?;
                    }
                    Err(anyhow!("Pre-submission dry-run failed. Use --skip-dry-run to skip this step."))
                }
            }
        }
    }

    pub fn contract(&self) -> &<DefaultConfig as Config>::AccountId {
        &self.contract
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn args(&self) -> &Vec<String> {
        &self.args
    }

    pub fn opts(&self) -> &ExtrinsicOpts {
        &self.opts
    }

    pub fn gas_limit(&self) -> Option<u64> {
        self.gas_limit
    }

    pub fn proof_size(&self) -> Option<u64> {
        self.proof_size
    }

    pub fn value(&self) -> &BalanceVariant {
        &self.value
    }

    pub fn output_json(&self) -> bool {
        self.output_json
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn transcoder(&self) -> &ContractMessageTranscoder {
        &self.transcoder
    }

    pub fn call_data(&self) -> &Vec<u8> {
        &self.call_data
    }

    pub fn signer(&self) -> &Keypair {
        &self.signer
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
        name_value_println!("Result", format!("{}", self.data), DEFAULT_KEY_COL_WIDTH);
        name_value_println!(
            "Reverted",
            format!("{:?}", self.reverted),
            DEFAULT_KEY_COL_WIDTH
        );
    }
}

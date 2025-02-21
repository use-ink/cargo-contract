// Copyright (C) Use Ink (UK) Ltd.
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
    pallet_revive_primitives::ContractExecResult,
    state_call,
    submit_extrinsic,
    ContractMessageTranscoder,
    ErrorVariant,
};
use crate::{
    check_env_types,
    extrinsic_calls::Call,
    extrinsic_opts::ExtrinsicOpts,
};

use anyhow::{
    anyhow,
    Result,
};
use ink_env::Environment;
use scale::Encode;
use sp_runtime::traits::Zero;
use sp_weights::Weight;

use crate::pallet_revive_primitives::StorageDeposit;
use subxt::{
    backend::{
        legacy::LegacyRpcMethods,
        rpc::RpcClient,
    },
    blocks::ExtrinsicEvents,
    config::{
        DefaultExtrinsicParams,
        ExtrinsicParams,
    },
    ext::{
        scale_decode::IntoVisitor,
        scale_encode::EncodeAsType,
    },
    tx,
    utils::H160,
    Config,
    OnlineClient,
};

/// A builder for the call command.
pub struct CallCommandBuilder<C: Config, E: Environment, Signer: Clone> {
    contract: H160,
    message: String,
    args: Vec<String>,
    extrinsic_opts: ExtrinsicOpts<C, E, Signer>,
    gas_limit: Option<u64>,
    proof_size: Option<u64>,
    value: E::Balance,
}

impl<C: Config, E: Environment, Signer> CallCommandBuilder<C, E, Signer>
where
    E::Balance: Default,
    Signer: tx::Signer<C> + Clone,
{
    /// Returns a clean builder for [`CallExec`].
    pub fn new(
        contract: H160,
        message: &str,
        extrinsic_opts: ExtrinsicOpts<C, E, Signer>,
    ) -> CallCommandBuilder<C, E, Signer> {
        CallCommandBuilder {
            contract,
            message: message.to_string(),
            args: Vec::new(),
            extrinsic_opts,
            gas_limit: None,
            proof_size: None,
            value: Default::default(),
        }
    }

    /// Sets the arguments of the contract message to call.
    pub fn args<T: ToString>(self, args: Vec<T>) -> Self {
        let mut this = self;
        this.args = args.into_iter().map(|arg| arg.to_string()).collect();
        this
    }

    /// Sets the maximum amount of gas to be used for this command.
    pub fn gas_limit(self, gas_limit: Option<u64>) -> Self {
        let mut this = self;
        this.gas_limit = gas_limit;
        this
    }

    /// Sets the maximum proof size for this call.
    pub fn proof_size(self, proof_size: Option<u64>) -> Self {
        let mut this = self;
        this.proof_size = proof_size;
        this
    }

    /// Sets the value to be transferred as part of the call.
    pub fn value(self, value: E::Balance) -> Self {
        let mut this = self;
        this.value = value;
        this
    }

    /// Preprocesses contract artifacts and options for subsequent contract calls.
    ///
    /// This function prepares the necessary data for making a contract call based on the
    /// provided contract artifacts, message, arguments, and options. It ensures that the
    /// required contract code and message data are available, sets up the client,
    /// and other relevant parameters, preparing for the contract call operation.
    ///
    /// Returns the `CallExec` containing the preprocessed data for the contract call,
    /// or an error in case of failure.
    pub async fn done(self) -> Result<CallExec<C, E, Signer>> {
        let artifacts = self.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;

        let call_data = transcoder.encode(&self.message, &self.args)?;
        tracing::debug!("Message data: {:?}", hex::encode(&call_data));

        let url = self.extrinsic_opts.url();
        let rpc = RpcClient::from_url(&url).await?;
        let client = OnlineClient::from_rpc_client(rpc.clone()).await?;
        let rpc = LegacyRpcMethods::new(rpc);
        check_env_types(&client, &transcoder, self.extrinsic_opts.verbosity())?;

        Ok(CallExec {
            contract: self.contract,
            message: self.message.clone(),
            args: self.args.clone(),
            opts: self.extrinsic_opts,
            gas_limit: self.gas_limit,
            proof_size: self.proof_size,
            value: self.value,
            rpc,
            client,
            transcoder,
            call_data,
        })
    }
}

pub struct CallExec<C: Config, E: Environment, Signer: Clone> {
    contract: H160,
    message: String,
    args: Vec<String>,
    opts: ExtrinsicOpts<C, E, Signer>,
    gas_limit: Option<u64>,
    proof_size: Option<u64>,
    value: E::Balance,
    rpc: LegacyRpcMethods<C>,
    client: OnlineClient<C>,
    transcoder: ContractMessageTranscoder,
    call_data: Vec<u8>,
}

impl<C: Config, E: Environment, Signer> CallExec<C, E, Signer>
where
    <C::ExtrinsicParams as ExtrinsicParams<C>>::Params:
        From<<DefaultExtrinsicParams<C> as ExtrinsicParams<C>>::Params>,
    C::AccountId: EncodeAsType + IntoVisitor,
    E::Balance: EncodeAsType + Zero,
    Signer: tx::Signer<C> + Clone,
{
    /// Simulates a contract call without modifying the blockchain.
    ///
    /// This function performs a dry run simulation of a contract call, capturing
    /// essential information such as the contract address, gas consumption, and
    /// storage deposit. The simulation is executed without actually executing the
    /// call on the blockchain.
    ///
    /// Returns the dry run simulation result of type [`ContractExecResult`], which
    /// includes information about the simulated call, or an error in case of failure.
    pub async fn call_dry_run(&self) -> Result<ContractExecResult<E::Balance>> {
        let storage_deposit_limit = self.opts.storage_deposit_limit();
        let call_request = CallRequest {
            origin: self.opts.signer().account_id(),
            dest: self.contract,
            value: self.value,
            gas_limit: None,
            storage_deposit_limit,
            input_data: self.call_data.clone(),
        };
        state_call(&self.rpc, "ReviveApi_call", call_request).await
    }

    /// Calls a contract on the blockchain with a specified gas limit.
    ///
    /// This function facilitates the process of invoking a contract, specifying the gas
    /// limit for the operation. It interacts with the blockchain's runtime API to
    /// execute the contract call and provides the resulting events from the call.
    ///
    /// Returns the events generated from the contract call, or an error in case of
    /// failure.
    pub async fn call(
        &self,
        gas_limit: Option<Weight>,
        storage_deposit_limit: Option<E::Balance>,
    ) -> Result<ExtrinsicEvents<C>, ErrorVariant> {
        if !self
            .transcoder()
            .metadata()
            .spec()
            .messages()
            .iter()
            .find(|msg| msg.label() == &self.message)
            .expect("message exist after calling CallExec::done()")
            .mutates()
        {
            let inner = anyhow!(
                "Tried to execute a call on the immutable contract message '{}'. Please do a dry-run instead.",
                &self.message
            );
            return Err(inner.into())
        }

        // use user specified values where provided, otherwise estimate
        // todo write in a way that estimate_gas() is only executed when really needed
        let estimate = self.estimate_gas().await?;
        let gas_limit = match gas_limit {
            Some(gas_limit) => gas_limit,
            None => estimate.0,
        };
        let storage_deposit_limit = match storage_deposit_limit {
            Some(deposit_limit) => deposit_limit,
            None => estimate.1,
        };
        tracing::debug!("calling contract {:?}", self.contract);

        let call = Call::new(
            self.contract,
            self.value,
            gas_limit,
            storage_deposit_limit,
            self.call_data.clone(),
        )
        .build();

        let result =
            submit_extrinsic(&self.client, &self.rpc, &call, self.opts.signer()).await?;

        Ok(result)
    }

    /// Estimates the gas required for a contract call without modifying the blockchain.
    ///
    /// This function provides a gas estimation for contract calls, considering the
    /// user-specified values or using estimates based on a dry run. The estimated gas
    /// weight is returned, or an error is reported if the estimation fails.
    ///
    /// Returns the estimated gas weight of type [`Weight`] for contract calls, or an
    /// error.
    pub async fn estimate_gas(&self) -> Result<(Weight, E::Balance)> {
        match (
            self.gas_limit,
            self.proof_size,
            self.opts.storage_deposit_limit(),
        ) {
            (Some(ref_time), Some(proof_size), Some(deposit_limit)) => {
                Ok((Weight::from_parts(ref_time, proof_size), deposit_limit))
            }
            _ => {
                let call_result = self.call_dry_run().await?;
                match call_result.result {
                    Ok(_) => {
                        // use user specified values where provided, otherwise use the
                        // estimates
                        let ref_time = self
                            .gas_limit
                            .unwrap_or_else(|| call_result.gas_required.ref_time());
                        let proof_size = self
                            .proof_size
                            .unwrap_or_else(|| call_result.gas_required.proof_size());
                        let storage_deposit_limit =
                            self.opts.storage_deposit_limit().unwrap_or_else(|| {
                                match call_result.storage_deposit {
                                    StorageDeposit::Refund(_) => E::Balance::zero(),
                                    StorageDeposit::Charge(charge) => charge,
                                }
                            });
                        Ok((
                            Weight::from_parts(ref_time, proof_size),
                            storage_deposit_limit,
                        ))
                    }
                    Err(ref err) => {
                        let object = ErrorVariant::from_dispatch_error(
                            err,
                            &self.client.metadata(),
                        )?;
                        Err(anyhow!("Pre-submission dry-run failed. Error: {}", object))
                    }
                }
            }
        }
    }

    /// Returns the address of the the contract to call.
    pub fn contract(&self) -> &subxt::utils::H160 {
        &self.contract
    }

    /// Returns the name of the contract message to call.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the arguments of the contract message to call.
    pub fn args(&self) -> &Vec<String> {
        &self.args
    }

    /// Returns the extrinsic options.
    pub fn opts(&self) -> &ExtrinsicOpts<C, E, Signer> {
        &self.opts
    }

    /// Returns the maximum amount of gas to be used for this command.
    pub fn gas_limit(&self) -> Option<u64> {
        self.gas_limit
    }

    /// Returns the maximum proof size for this call.
    pub fn proof_size(&self) -> Option<u64> {
        self.proof_size
    }

    /// Returns the value to be transferred as part of the call.
    pub fn value(&self) -> &E::Balance {
        &self.value
    }

    /// Returns the client.
    pub fn client(&self) -> &OnlineClient<C> {
        &self.client
    }

    /// Returns the rpc.
    pub fn rpc(&self) -> &LegacyRpcMethods<C> {
        &self.rpc
    }

    /// Returns the contract message transcoder.
    pub fn transcoder(&self) -> &ContractMessageTranscoder {
        &self.transcoder
    }

    /// Returns the call data.
    pub fn call_data(&self) -> &Vec<u8> {
        &self.call_data
    }
}

/// A struct that encodes RPC parameters required for a call to a smart contract.
///
/// Copied from `pallet-contracts-rpc-runtime-api`.
#[derive(Encode)]
struct CallRequest<AccountId, Balance> {
    origin: AccountId,
    dest: subxt::utils::H160,
    value: Balance,
    gas_limit: Option<Weight>,
    storage_deposit_limit: Option<Balance>,
    input_data: Vec<u8>,
}

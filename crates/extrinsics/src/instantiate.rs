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
    fetch_contract_binary,
    get_account_nonce,
    pallet_revive_primitives::{
        ContractInstantiateResult,
        StorageDeposit,
    },
    state_call,
    submit_extrinsic,
    AccountIdMapper,
    ContractMessageTranscoder,
    ErrorVariant,
};
use crate::{
    check_env_types,
    extrinsic_calls::{
        Instantiate,
        InstantiateWithCode,
    },
    extrinsic_opts::ExtrinsicOpts,
};
use anyhow::{
    anyhow,
    Context,
    Result,
};
use contract_transcode::Value;
use ink_env::Environment;
use serde::Serialize;

use scale::{
    Decode,
    Encode,
};
use sp_core::Bytes;
use sp_runtime::{
    traits::Zero,
    SaturatedConversion,
};
use sp_weights::Weight;
use std::fmt::Display;
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
    utils::{
        H160,
        H256,
    },
    Config,
    OnlineClient,
};

/// A builder for the instantiate command.
pub struct InstantiateCommandBuilder<C: Config, E: Environment, Signer: Clone> {
    constructor: String,
    args: Vec<String>,
    extrinsic_opts: ExtrinsicOpts<C, E, Signer>,
    value: E::Balance,
    gas_limit: Option<u64>,
    proof_size: Option<u64>,
    salt: Option<Bytes>,
}

impl<C: Config, E: Environment, Signer> InstantiateCommandBuilder<C, E, Signer>
where
    E::Balance: Default,
    Signer: tx::Signer<C> + Clone,
    C::Hash: From<[u8; 32]>,
{
    /// Returns a clean builder for [`InstantiateExec`].
    pub fn new(
        extrinsic_opts: ExtrinsicOpts<C, E, Signer>,
    ) -> InstantiateCommandBuilder<C, E, Signer> {
        InstantiateCommandBuilder {
            constructor: String::from("new"),
            args: Vec::new(),
            extrinsic_opts,
            value: Default::default(),
            gas_limit: None,
            proof_size: None,
            salt: None,
        }
    }

    /// Sets the name of the contract constructor to call.
    pub fn constructor<T: Into<String>>(self, constructor: T) -> Self {
        let mut this = self;
        this.constructor = constructor.into();
        this
    }

    /// Sets the constructor arguments.
    pub fn args<T: ToString>(self, args: Vec<T>) -> Self {
        let mut this = self;
        this.args = args.into_iter().map(|arg| arg.to_string()).collect();
        this
    }

    /// Sets the initial balance to transfer to the instantiated contract.
    pub fn value(self, value: E::Balance) -> Self {
        let mut this = self;
        this.value = value;
        this
    }

    /// Sets the maximum amount of gas to be used for this command.
    pub fn gas_limit(self, gas_limit: Option<u64>) -> Self {
        let mut this = self;
        this.gas_limit = gas_limit;
        this
    }

    /// Sets the maximum proof size for this instantiation.
    pub fn proof_size(self, proof_size: Option<u64>) -> Self {
        let mut this = self;
        this.proof_size = proof_size;
        this
    }

    /// Sets the salt used in the address derivation of the new contract.
    pub fn salt(self, salt: Option<Bytes>) -> Self {
        let mut this = self;
        this.salt = salt;
        this
    }

    /// Preprocesses contract artifacts and options for instantiation.
    ///
    /// This function prepares the required data for instantiating a contract based on the
    /// provided contract artifacts and options. It ensures that the necessary contract
    /// code is available, sets up the client, signer, and other relevant parameters,
    /// preparing for the instantiation process.
    ///
    /// Returns the [`InstantiateExec`] containing the preprocessed data for the
    /// instantiation, or an error in case of failure.
    pub async fn done(self) -> Result<InstantiateExec<C, E, Signer>> {
        let artifacts = self.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;
        let data = transcoder.encode(&self.constructor, &self.args)?;
        let url = self.extrinsic_opts.url();
        let code = if let Some(code) = artifacts.contract_binary {
            Code::Upload(code.0)
        } else {
            let code_hash = artifacts.code_hash()?;
            Code::Existing(code_hash.into())
        };
        let salt = self.salt.clone().map(|s| {
            let bytes = s.0;
            assert!(bytes.len() <= 32, "salt has to be <= 32 bytes");
            let mut salt = [0u8; 32];
            salt[..bytes.len()].copy_from_slice(&bytes[..bytes.len()]);
            salt
        });

        let rpc_cli = RpcClient::from_url(&url).await?;
        let client = OnlineClient::from_rpc_client(rpc_cli.clone()).await?;
        check_env_types(&client, &transcoder, self.extrinsic_opts.verbosity())?;
        let rpc = LegacyRpcMethods::new(rpc_cli);

        let args = InstantiateArgs {
            constructor: self.constructor.clone(),
            raw_args: self.args.clone(),
            value: self.value,
            gas_limit: self.gas_limit,
            proof_size: self.proof_size,
            storage_deposit_limit: self.extrinsic_opts.storage_deposit_limit(),
            code,
            data,
            salt,
        };

        Ok(InstantiateExec {
            args,
            opts: self.extrinsic_opts,
            rpc,
            client,
            transcoder,
        })
    }
}

pub struct InstantiateArgs<E: Environment> {
    constructor: String,
    raw_args: Vec<String>,
    value: E::Balance,
    gas_limit: Option<u64>,
    proof_size: Option<u64>,
    storage_deposit_limit: Option<E::Balance>,
    code: Code,
    data: Vec<u8>,
    salt: Option<[u8; 32]>,
}

impl<E: Environment> InstantiateArgs<E> {
    /// Returns the constructor name.
    pub fn constructor(&self) -> &str {
        &self.constructor
    }

    /// Returns the constructor raw arguments.
    pub fn raw_args(&self) -> &[String] {
        &self.raw_args
    }

    /// Returns the value to transfer to the instantiated contract.
    pub fn value(&self) -> E::Balance {
        self.value
    }

    /// Returns the maximum amount of gas to be used for this command.
    pub fn gas_limit(&self) -> Option<u64> {
        self.gas_limit
    }

    /// Returns the maximum proof size for this instantiation.
    pub fn proof_size(&self) -> Option<u64> {
        self.proof_size
    }

    /// Returns the storage deposit limit for this instantiation.
    pub fn storage_deposit_limit(&self) -> Option<E::Balance> {
        self.storage_deposit_limit
    }

    pub fn code(&self) -> &Code {
        &self.code
    }

    /// Returns the input data for the contract constructor.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Returns the salt used in the address derivation of the new contract.
    pub fn salt(&self) -> Option<&[u8; 32]> {
        self.salt.as_ref()
    }
}

pub struct InstantiateExec<C: Config, E: Environment, Signer: Clone> {
    opts: ExtrinsicOpts<C, E, Signer>,
    args: InstantiateArgs<E>,
    rpc: LegacyRpcMethods<C>,
    client: OnlineClient<C>,
    transcoder: ContractMessageTranscoder,
}

impl<C: Config, E: Environment, Signer> InstantiateExec<C, E, Signer>
where
    C::AccountId: Decode,
    <C::ExtrinsicParams as ExtrinsicParams<C>>::Params:
        From<<DefaultExtrinsicParams<C> as ExtrinsicParams<C>>::Params>,
    C::Hash: IntoVisitor + EncodeAsType,
    C::AccountId: IntoVisitor + Display,
    E::Balance: IntoVisitor + Serialize + EncodeAsType + Zero,
    Signer: tx::Signer<C> + Clone,
{
    /// Decodes the result of a simulated contract instantiation.
    ///
    /// This function decodes the result of a simulated contract instantiation dry run.
    /// It processes the returned data, including the constructor's return value, contract
    /// address, gas consumption, and storage deposit, and packages them into an
    /// [`InstantiateDryRunResult`].
    ///
    /// Returns the decoded dry run result, or an error in case of failure.
    pub async fn decode_instantiate_dry_run(
        &self,
        result: &ContractInstantiateResult<E::Balance>,
    ) -> Result<InstantiateDryRunResult<E::Balance>, ErrorVariant> {
        tracing::debug!("instantiate data {:?}", self.args.data);
        match result.result {
            Ok(ref ret_val) => {
                let value = self
                    .transcoder
                    .decode_constructor_return(
                        &self.args.constructor,
                        &mut &ret_val.result.data[..],
                    )
                    .context(format!("Failed to decode return value {:?}", &ret_val))?;
                let dry_run_result = InstantiateDryRunResult {
                    result: value,
                    contract: ret_val.account_id,
                    reverted: ret_val.result.did_revert(),
                    gas_consumed: result.gas_consumed,
                    gas_required: result.gas_required,
                    storage_deposit: result.storage_deposit.clone(),
                };
                Ok(dry_run_result)
            }
            Err(ref err) => {
                let metadata = self.client.metadata();
                Err(ErrorVariant::from_dispatch_error(err, &metadata)?)
            }
        }
    }

    /// Simulates a contract instantiation without modifying the blockchain.
    ///
    /// This function performs a dry run simulation of a contract instantiation, capturing
    /// essential information such as the contract address, gas consumption, and storage
    /// deposit. The simulation is executed without actually executing the
    /// instantiation on the blockchain.
    ///
    /// Returns the dry run simulation result, or an error in case of failure.
    pub async fn instantiate_dry_run(
        &self,
    ) -> Result<ContractInstantiateResult<E::Balance>> {
        let call_request = InstantiateRequest::<C, E> {
            origin: self.opts.signer().account_id(),
            value: self.args.value,
            gas_limit: None,
            storage_deposit_limit: self.args.storage_deposit_limit,
            code: self.args.code.clone(),
            data: self.args.data.clone(),
            salt: self.args.salt,
        };
        state_call(&self.rpc, "ReviveApi_instantiate", &call_request).await
    }

    async fn instantiate_with_code(
        &self,
        code: Vec<u8>,
        gas_limit: Weight,
        storage_deposit_limit: E::Balance,
    ) -> Result<InstantiateExecResult<C>, ErrorVariant> {
        let code_hash = None; // todo
        let contract_address = contract_address(
            &self.client,
            &self.rpc,
            self.opts.signer(),
            &self.args.salt,
            &code[..],
            &self.args.data[..],
        )
        .await?;

        let call = InstantiateWithCode::new(
            self.args.value,
            gas_limit,
            storage_deposit_limit,
            code,
            self.args.data.clone(),
            self.args.salt.map(Into::into).clone(),
        )
        .build();

        let events =
            submit_extrinsic(&self.client, &self.rpc, &call, self.opts.signer()).await?;

        Ok(InstantiateExecResult {
            events,
            code_hash,
            contract_address,
        })
    }

    async fn instantiate_with_code_hash(
        &self,
        code_hash: H256,
        gas_limit: Weight,
        storage_deposit_limit: E::Balance,
    ) -> Result<InstantiateExecResult<C>, ErrorVariant> {
        let call = Instantiate::<E::Balance>::new(
            self.args.value,
            gas_limit,
            storage_deposit_limit,
            code_hash,
            self.args.data.clone(),
            self.args.salt,
        )
        .build();

        let events =
            submit_extrinsic(&self.client, &self.rpc, &call, self.opts.signer()).await?;

        let code = fetch_contract_binary(&self.client, &self.rpc, &code_hash).await?;
        let contract_address = contract_address(
            &self.client,
            &self.rpc,
            self.opts.signer(),
            &self.args.salt,
            &code[..],
            &self.args.data[..],
        )
        .await?;
        Ok(InstantiateExecResult {
            events,
            code_hash: None,
            contract_address,
        })
    }

    /// Initiates the deployment of a smart contract on the blockchain.
    ///
    /// This function can be used to deploy a contract using either its source code or an
    /// existing code hash. It triggers the instantiation process by submitting an
    /// extrinsic with the specified gas limit, storage deposit, code or code hash,
    /// input data, and salt.
    ///
    /// The deployment result provides essential information about the instantiation,
    /// encapsulated in an [`InstantiateExecResult`] object, including the contract's
    /// result, contract address, and token metadata.
    pub async fn instantiate(
        &self,
        gas_limit: Option<Weight>,
        storage_deposit_limit: Option<E::Balance>,
    ) -> Result<InstantiateExecResult<C>, ErrorVariant> {
        // use user specified values where provided, otherwise estimate
        let use_gas_limit;
        let use_storage_deposit_limit;
        if gas_limit.is_none() || storage_deposit_limit.is_none() {
            let estimation = self.estimate_limits().await?;
            if gas_limit.is_none() {
                use_gas_limit = estimation.0;
            } else {
                use_gas_limit = gas_limit.unwrap();
            }
            if storage_deposit_limit.is_none() {
                use_storage_deposit_limit = estimation.1;
            } else {
                use_storage_deposit_limit = storage_deposit_limit.unwrap();
            }
        } else {
            use_gas_limit = gas_limit.unwrap();
            use_storage_deposit_limit = storage_deposit_limit.unwrap();
        }

        match self.args.code.clone() {
            Code::Upload(code) => {
                self.instantiate_with_code(code, use_gas_limit, use_storage_deposit_limit)
                    .await
            }
            Code::Existing(code_hash) => {
                self.instantiate_with_code_hash(
                    code_hash,
                    use_gas_limit,
                    use_storage_deposit_limit,
                )
                .await
            }
        }
    }

    /// Estimates the gas required for the contract instantiation process without
    /// modifying the blockchain.
    ///
    /// This function provides a gas estimation for contract instantiation, considering
    /// the user-specified values or using estimates based on a dry run.
    ///
    /// Returns the estimated gas weight of type [`Weight`] for contract instantiation, or
    /// an error.
    pub async fn estimate_limits(&self) -> Result<(Weight, E::Balance)> {
        let instantiate_result = self.instantiate_dry_run().await?;
        match instantiate_result.result {
            Ok(_) => {
                // use user specified values where provided, otherwise use the
                // estimates
                let ref_time = self
                    .args
                    .gas_limit
                    .unwrap_or_else(|| instantiate_result.gas_required.ref_time());
                let proof_size = self
                    .args
                    .proof_size
                    .unwrap_or_else(|| instantiate_result.gas_required.proof_size());
                let deposit_limit =
                    self.args.storage_deposit_limit.unwrap_or_else(|| {
                        match instantiate_result.storage_deposit {
                            StorageDeposit::Refund(_) => E::Balance::zero(),
                            StorageDeposit::Charge(value) => value,
                        }
                    });
                Ok((Weight::from_parts(ref_time, proof_size), deposit_limit))
            }
            Err(ref err) => {
                let object =
                    ErrorVariant::from_dispatch_error(err, &self.client.metadata())?;
                tracing::info!("Pre-submission dry-run failed. Error: {}", object);
                Err(anyhow!("Pre-submission dry-run failed. Error: {}", object))
            }
        }
    }

    /*
    /// Estimates the gas required for the contract instantiation process without
    /// modifying the blockchain.
    ///
    /// This function provides a gas estimation for contract instantiation, considering
    /// the user-specified values or using estimates based on a dry run.
    ///
    /// Returns the estimated gas weight of type [`Weight`] for contract instantiation, or
    /// an error.
    pub async fn estimate_gas(&self) -> Result<Weight> {
        match (self.args.gas_limit, self.args.proof_size) {
            (Some(ref_time), Some(proof_size)) => {
                Ok(Weight::from_parts(ref_time, proof_size))
            }
            _ => {
                let instantiate_result = self.instantiate_dry_run().await?;
                match instantiate_result.result {
                    Ok(_) => {
                        // use user specified values where provided, otherwise use the
                        // estimates
                        let ref_time = self.args.gas_limit.unwrap_or_else(|| {
                            instantiate_result.gas_required.ref_time()
                        });
                        let proof_size = self.args.proof_size.unwrap_or_else(|| {
                            instantiate_result.gas_required.proof_size()
                        });
                        Ok(Weight::from_parts(ref_time, proof_size))
                    }
                    Err(ref err) => {
                        let object = ErrorVariant::from_dispatch_error(
                            err,
                            &self.client.metadata(),
                        )?;
                        tracing::info!(
                            "Pre-submission dry-run failed. Error: {}",
                            object
                        );
                        Err(anyhow!("Pre-submission dry-run failed. Error: {}", object))
                    }
                }
            }
        }
    }
    */

    /// Returns the extrinsic options.
    pub fn opts(&self) -> &ExtrinsicOpts<C, E, Signer> {
        &self.opts
    }

    /// Returns the instantiate arguments.
    pub fn args(&self) -> &InstantiateArgs<E> {
        &self.args
    }

    /// Returns the client.
    pub fn client(&self) -> &OnlineClient<C> {
        &self.client
    }

    /// Returns the interface to call the legacy RPC methods.
    pub fn rpc(&self) -> &LegacyRpcMethods<C> {
        &self.rpc
    }

    /// Returns the contract message transcoder.
    pub fn transcoder(&self) -> &ContractMessageTranscoder {
        &self.transcoder
    }
}

/// A struct representing the result of an instantiate command execution.
pub struct InstantiateExecResult<C: Config> {
    pub events: ExtrinsicEvents<C>,
    pub code_hash: Option<H256>,
    pub contract_address: H160,
}

/// Result of the contract call
#[derive(serde::Serialize)]
pub struct InstantiateDryRunResult<Balance: Serialize> {
    /// The decoded result returned from the constructor
    pub result: Value,
    /// Contract address
    pub contract: H160,
    /// Was the operation reverted
    pub reverted: bool,
    pub gas_consumed: Weight,
    pub gas_required: Weight,
    /// Storage deposit after the operation
    pub storage_deposit: StorageDeposit<Balance>,
}

impl<Balance: Serialize> InstantiateDryRunResult<Balance> {
    /// Returns a result in json format
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

/// A struct that encodes RPC parameters required to instantiate a new smart contract.
#[derive(Encode)]
struct InstantiateRequest<C: Config, E: Environment> {
    origin: C::AccountId,
    value: E::Balance,
    gas_limit: Option<Weight>,
    storage_deposit_limit: Option<E::Balance>,
    code: Code,
    data: Vec<u8>,
    salt: Option<[u8; 32]>,
}

/// Reference to an existing code hash or new contract binary.
#[derive(Clone, Encode)]
pub enum Code {
    /// A contract binary as raw bytes.
    Upload(Vec<u8>),
    /// The code hash of an on-chain contract binary blob.
    Existing(H256),
}

/// Derives a contract address.
pub async fn contract_address<C: Config, Signer: tx::Signer<C> + Clone>(
    client: &OnlineClient<C>,
    rpc: &LegacyRpcMethods<C>,
    signer: &Signer,
    salt: &Option<[u8; 32]>,
    code: &[u8],
    data: &[u8],
) -> Result<H160, subxt::Error> {
    let account_id = Signer::account_id(signer);
    let deployer = AccountIdMapper::to_address(&account_id.encode()[..]);

    // copied from `pallet-revive`
    let origin_is_caller = false;
    let addr = if let Some(salt) = salt {
        pallet_revive::create2(&deployer, code, data, salt)
    } else {
        let account_nonce = get_account_nonce(client, rpc, &account_id).await?;
        pallet_revive::create1(
            &deployer,
            // the Nonce from the origin has been incremented pre-dispatch, so we
            // need to subtract 1 to get the nonce at the time of the call.
            if origin_is_caller {
                account_nonce.saturating_sub(1u32.into()).saturated_into()
            } else {
                account_nonce.saturated_into()
            },
        )
    };
    Ok(addr)
}

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
    events::{
        CodeStored,
        ContractInstantiated,
    },
    state,
    state_call,
    submit_extrinsic,
    Balance,
    BalanceVariant,
    Client,
    CodeHash,
    ContractMessageTranscoder,
    DefaultConfig,
    ErrorVariant,
    Missing,
    StorageDeposit,
    TokenMetadata,
    Weight,
};
use crate::{
    check_env_types,
    extrinsic_opts::ExtrinsicOpts,
};
use anyhow::{
    anyhow,
    Context,
    Result,
};
use contract_transcode::Value;
use subxt_signer::sr25519::Keypair;

use pallet_contracts_primitives::ContractInstantiateResult;

use core::marker::PhantomData;
use scale::Encode;
use sp_core::Bytes;
use subxt::{
    backend::{
        legacy::LegacyRpcMethods,
        rpc::RpcClient,
    },
    blocks::ExtrinsicEvents,
    ext::scale_encode,
    Config,
    OnlineClient,
};

struct InstantiateOpts {
    constructor: String,
    args: Vec<String>,
    extrinsic_opts: ExtrinsicOpts,
    value: BalanceVariant,
    gas_limit: Option<u64>,
    proof_size: Option<u64>,
    salt: Option<Bytes>,
}

/// A builder for the instantiate command.
pub struct InstantiateCommandBuilder<ExtrinsicOptions> {
    opts: InstantiateOpts,
    marker: PhantomData<fn() -> ExtrinsicOptions>,
}

impl InstantiateCommandBuilder<Missing<state::ExtrinsicOptions>> {
    /// Returns a clean builder for [`InstantiateExec`].
    pub fn new() -> InstantiateCommandBuilder<Missing<state::ExtrinsicOptions>> {
        InstantiateCommandBuilder {
            opts: InstantiateOpts {
                constructor: String::from("new"),
                args: Vec::new(),
                extrinsic_opts: ExtrinsicOpts::default(),
                value: "0".parse().unwrap(),
                gas_limit: None,
                proof_size: None,
                salt: None,
            },
            marker: PhantomData,
        }
    }

    /// Sets the extrinsic operation.
    pub fn extrinsic_opts(
        self,
        extrinsic_opts: ExtrinsicOpts,
    ) -> InstantiateCommandBuilder<state::ExtrinsicOptions> {
        InstantiateCommandBuilder {
            opts: InstantiateOpts {
                extrinsic_opts,
                ..self.opts
            },
            marker: PhantomData,
        }
    }
}

impl Default for InstantiateCommandBuilder<Missing<state::ExtrinsicOptions>> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> InstantiateCommandBuilder<E> {
    /// Sets the name of the contract constructor to call.
    pub fn constructor<T: Into<String>>(self, constructor: T) -> Self {
        let mut this = self;
        this.opts.constructor = constructor.into();
        this
    }

    /// Sets the constructor arguments.
    pub fn args<T: ToString>(self, args: Vec<T>) -> Self {
        let mut this = self;
        this.opts.args = args.into_iter().map(|arg| arg.to_string()).collect();
        this
    }

    /// Sets the initial balance to transfer to the instantiated contract.
    pub fn value(self, value: BalanceVariant) -> Self {
        let mut this = self;
        this.opts.value = value;
        this
    }

    /// Sets the maximum amount of gas to be used for this command.
    pub fn gas_limit(self, gas_limit: Option<u64>) -> Self {
        let mut this = self;
        this.opts.gas_limit = gas_limit;
        this
    }

    /// Sets the maximum proof size for this instantiation.
    pub fn proof_size(self, proof_size: Option<u64>) -> Self {
        let mut this = self;
        this.opts.proof_size = proof_size;
        this
    }

    /// Sets the salt used in the address derivation of the new contract.
    pub fn salt(self, salt: Option<Bytes>) -> Self {
        let mut this = self;
        this.opts.salt = salt;
        this
    }
}

impl InstantiateCommandBuilder<state::ExtrinsicOptions> {
    /// Preprocesses contract artifacts and options for instantiation.
    ///
    /// This function prepares the required data for instantiating a contract based on the
    /// provided contract artifacts and options. It ensures that the necessary contract
    /// code is available, sets up the client, signer, and other relevant parameters,
    /// preparing for the instantiation process.
    ///
    /// Returns the [`InstantiateExec`] containing the preprocessed data for the
    /// instantiation, or an error in case of failure.
    pub async fn done(self) -> Result<InstantiateExec> {
        let artifacts = self.opts.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;
        let data = transcoder.encode(&self.opts.constructor, &self.opts.args)?;
        let signer = self.opts.extrinsic_opts.signer()?;
        let url = self.opts.extrinsic_opts.url();
        let code = if let Some(code) = artifacts.code {
            Code::Upload(code.0)
        } else {
            let code_hash = artifacts.code_hash()?;
            Code::Existing(code_hash.into())
        };
        let salt = self.opts.salt.clone().map(|s| s.0).unwrap_or_default();

        let rpc_cli = RpcClient::from_url(&url).await?;
        let client = OnlineClient::from_rpc_client(rpc_cli.clone()).await?;
        check_env_types(&client, &transcoder)?;
        let rpc = LegacyRpcMethods::new(rpc_cli);

        let token_metadata = TokenMetadata::query(&rpc).await?;

        let args = InstantiateArgs {
            constructor: self.opts.constructor.clone(),
            raw_args: self.opts.args.clone(),
            value: self.opts.value.denominate_balance(&token_metadata)?,
            gas_limit: self.opts.gas_limit,
            proof_size: self.opts.proof_size,
            storage_deposit_limit: self
                .opts
                .extrinsic_opts
                .storage_deposit_limit()
                .as_ref()
                .map(|bv| bv.denominate_balance(&token_metadata))
                .transpose()?,
            code,
            data,
            salt,
        };

        Ok(InstantiateExec {
            args,
            opts: self.opts.extrinsic_opts.clone(),
            url,
            rpc,
            client,
            signer,
            transcoder,
            token_metadata,
        })
    }
}

pub struct InstantiateArgs {
    constructor: String,
    raw_args: Vec<String>,
    value: Balance,
    gas_limit: Option<u64>,
    proof_size: Option<u64>,
    storage_deposit_limit: Option<Balance>,
    code: Code,
    data: Vec<u8>,
    salt: Vec<u8>,
}

impl InstantiateArgs {
    /// Returns the constructor name.
    pub fn constructor(&self) -> &str {
        &self.constructor
    }

    /// Returns the constructor raw arguments.
    pub fn raw_args(&self) -> &[String] {
        &self.raw_args
    }

    /// Returns the value to transfer to the instantiated contract.
    pub fn value(&self) -> Balance {
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
    pub fn storage_deposit_limit_compact(&self) -> Option<scale::Compact<Balance>> {
        self.storage_deposit_limit.map(Into::into)
    }

    pub fn code(&self) -> &Code {
        &self.code
    }

    /// Returns the input data for the contract constructor.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Returns the salt used in the address derivation of the new contract.
    pub fn salt(&self) -> &[u8] {
        &self.salt
    }
}

pub struct InstantiateExec {
    opts: ExtrinsicOpts,
    args: InstantiateArgs,
    url: String,
    rpc: LegacyRpcMethods<DefaultConfig>,
    client: Client,
    signer: Keypair,
    transcoder: ContractMessageTranscoder,
    token_metadata: TokenMetadata,
}

impl InstantiateExec {
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
        result: &ContractInstantiateResult<
            <DefaultConfig as Config>::AccountId,
            Balance,
            (),
        >,
    ) -> Result<InstantiateDryRunResult, ErrorVariant> {
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
                    contract: ret_val.account_id.to_string(),
                    reverted: ret_val.result.did_revert(),
                    gas_consumed: result.gas_consumed.into(),
                    gas_required: result.gas_required.into(),
                    storage_deposit: StorageDeposit::from(&result.storage_deposit),
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
    ) -> Result<
        ContractInstantiateResult<<DefaultConfig as Config>::AccountId, Balance, ()>,
    > {
        let storage_deposit_limit = self.args.storage_deposit_limit;
        let call_request = InstantiateRequest {
            origin: account_id(&self.signer),
            value: self.args.value,
            gas_limit: None,
            storage_deposit_limit,
            code: self.args.code.clone(),
            data: self.args.data.clone(),
            salt: self.args.salt.clone(),
        };
        state_call(&self.rpc, "ContractsApi_instantiate", &call_request).await
    }

    async fn instantiate_with_code(
        &self,
        code: Vec<u8>,
        gas_limit: Weight,
    ) -> Result<InstantiateExecResult, ErrorVariant> {
        // TODO: check if should be used unvalidated
        let call = subxt::tx::Payload::new(
            "Contracts",
            "instantiate_with_code",
            InstantiateWithCode {
                value: self.args.value,
                gas_limit,
                storage_deposit_limit: self.args.storage_deposit_limit,
                code,
                data: self.args.data.clone(),
                salt: self.args.salt.clone(),
            },
        );

        let result =
            submit_extrinsic(&self.client, &self.rpc, &call, &self.signer).await?;

        // The CodeStored event is only raised if the contract has not already been
        // uploaded.
        let code_hash = result
            .find_first::<CodeStored>()?
            .map(|code_stored| code_stored.code_hash);

        let instantiated = result
            .find_last::<ContractInstantiated>()?
            .ok_or_else(|| anyhow!("Failed to find Instantiated event"))?;

        Ok(InstantiateExecResult {
            result,
            code_hash,
            contract_address: instantiated.contract,
            token_metadata: self.token_metadata.clone(),
        })
    }

    async fn instantiate_with_code_hash(
        &self,
        code_hash: CodeHash,
        gas_limit: Weight,
    ) -> Result<InstantiateExecResult, ErrorVariant> {
        let call = subxt::tx::Payload::new(
            "Contracts",
            "instantiate_with_code",
            Instantiate {
                value: self.args.value,
                gas_limit,
                storage_deposit_limit: self.args.storage_deposit_limit,
                code: code_hash,
                data: self.args.data.clone(),
                salt: self.args.salt.clone(),
            },
        );

        let result =
            submit_extrinsic(&self.client, &self.rpc, &call, &self.signer).await?;

        let instantiated = result
            .find_first::<ContractInstantiated>()?
            .ok_or_else(|| anyhow!("Failed to find Instantiated event"))?;

        Ok(InstantiateExecResult {
            result,
            code_hash: None,
            contract_address: instantiated.contract,
            token_metadata: self.token_metadata.clone(),
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
    ) -> Result<InstantiateExecResult, ErrorVariant> {
        // use user specified values where provided, otherwise estimate
        let gas_limit = match gas_limit {
            Some(gas_limit) => gas_limit,
            None => self.estimate_gas().await?,
        };
        match self.args.code.clone() {
            Code::Upload(code) => self.instantiate_with_code(code, gas_limit).await,
            Code::Existing(code_hash) => {
                self.instantiate_with_code_hash(code_hash, gas_limit).await
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
                        Err(anyhow!("Pre-submission dry-run failed. Error: {}", object))
                    }
                }
            }
        }
    }

    /// Returns the extrinsic options.
    pub fn opts(&self) -> &ExtrinsicOpts {
        &self.opts
    }

    /// Returns the instantiate arguments.
    pub fn args(&self) -> &InstantiateArgs {
        &self.args
    }

    /// Returns the url.
    pub fn url(&self) -> &String {
        &self.url
    }

    /// Returns the client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Returns the signer.
    pub fn signer(&self) -> &Keypair {
        &self.signer
    }

    /// Returns the contract message transcoder.
    pub fn transcoder(&self) -> &ContractMessageTranscoder {
        &self.transcoder
    }
}

pub struct InstantiateExecResult {
    pub result: ExtrinsicEvents<DefaultConfig>,
    pub code_hash: Option<CodeHash>,
    pub contract_address: subxt::utils::AccountId32,
    pub token_metadata: TokenMetadata,
}

/// Result of the contract call
#[derive(serde::Serialize)]
pub struct InstantiateDryRunResult {
    /// The decoded result returned from the constructor
    pub result: Value,
    /// contract address
    pub contract: String,
    /// Was the operation reverted
    pub reverted: bool,
    pub gas_consumed: Weight,
    pub gas_required: Weight,
    /// Storage deposit after the operation
    pub storage_deposit: StorageDeposit,
}

impl InstantiateDryRunResult {
    /// Returns a result in json format
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

/// A struct that encodes RPC parameters required to instantiate a new smart contract.
#[derive(Encode)]
struct InstantiateRequest {
    origin: <DefaultConfig as Config>::AccountId,
    value: Balance,
    gas_limit: Option<Weight>,
    storage_deposit_limit: Option<Balance>,
    code: Code,
    data: Vec<u8>,
    salt: Vec<u8>,
}

/// Reference to an existing code hash or a new Wasm module.
#[derive(Clone, Encode)]
pub enum Code {
    /// A Wasm module as raw bytes.
    Upload(Vec<u8>),
    /// The code hash of an on-chain Wasm blob.
    Existing(<DefaultConfig as Config>::Hash),
}

/// A raw call to `pallet-contracts`'s `instantiate_with_code`.
#[derive(Debug, scale::Encode, scale::Decode, scale_encode::EncodeAsType)]
#[encode_as_type(trait_bounds = "", crate_path = "subxt::ext::scale_encode")]
pub struct InstantiateWithCode {
    #[codec(compact)]
    value: Balance,
    gas_limit: Weight,
    storage_deposit_limit: Option<Balance>,
    code: Vec<u8>,
    data: Vec<u8>,
    salt: Vec<u8>,
}

/// A raw call to `pallet-contracts`'s `instantiate_with_code_hash`.
#[derive(Debug, scale::Encode, scale::Decode, scale_encode::EncodeAsType)]
#[encode_as_type(trait_bounds = "", crate_path = "subxt::ext::scale_encode")]
pub struct Instantiate {
    #[codec(compact)]
    value: Balance,
    gas_limit: Weight,
    storage_deposit_limit: Option<Balance>,
    code: CodeHash,
    data: Vec<u8>,
    salt: Vec<u8>,
}

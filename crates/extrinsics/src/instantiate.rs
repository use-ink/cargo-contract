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
    Balance,
    BalanceVariant,
    Client,
    CodeHash,
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
    Context,
    Result,
};
use contract_build::{
    name_value_println,
    util::decode_hex,
    Verbosity,
};
use contract_transcode::Value;
use subxt_signer::sr25519::Keypair;

use pallet_contracts_primitives::ContractInstantiateResult;

use core::marker::PhantomData;
use scale::Encode;
use sp_core::Bytes;
use sp_weights::Weight;
use subxt::{
    blocks::ExtrinsicEvents,
    Config,
    OnlineClient,
};

#[derive(Debug, clap::Args)]
pub struct InstantiateCommand {
    /// The name of the contract constructor to call
    #[clap(name = "constructor", long, default_value = "new")]
    constructor: String,
    /// The constructor arguments, encoded as strings
    #[clap(long, num_args = 0..)]
    args: Vec<String>,
    #[clap(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Transfers an initial balance to the instantiated contract
    #[clap(name = "value", long, default_value = "0")]
    value: BalanceVariant,
    /// Maximum amount of gas to be used for this command.
    /// If not specified will perform a dry-run to estimate the gas consumed for the
    /// instantiation.
    #[clap(name = "gas", long)]
    gas_limit: Option<u64>,
    /// Maximum proof size for this instantiation.
    /// If not specified will perform a dry-run to estimate the proof size required.
    #[clap(long)]
    proof_size: Option<u64>,
    /// A salt used in the address derivation of the new contract. Use to create multiple
    /// instances of the same contract code from the same account.
    #[clap(long, value_parser = parse_hex_bytes)]
    salt: Option<Bytes>,
    /// Export the instantiate output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

/// Parse hex encoded bytes.
fn parse_hex_bytes(input: &str) -> Result<Bytes> {
    let bytes = decode_hex(input)?;
    Ok(bytes.into())
}

/// A builder for the instantiate command.
pub struct InstantiateCommandBuilder<ExtrinsicOptions> {
    opts: InstantiateCommand,
    marker: PhantomData<fn() -> ExtrinsicOptions>,
}

impl InstantiateCommandBuilder<Missing<state::ExtrinsicOptions>> {
    /// Sets the extrinsic operation.
    pub fn extrinsic_opts(
        self,
        extrinsic_opts: ExtrinsicOpts,
    ) -> InstantiateCommandBuilder<state::ExtrinsicOptions> {
        InstantiateCommandBuilder {
            opts: InstantiateCommand {
                extrinsic_opts,
                ..self.opts
            },
            marker: PhantomData,
        }
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
    pub fn args(self, args: Vec<String>) -> Self {
        let mut this = self;
        this.opts.args = args;
        this
    }

    /// Sets the initial balance to transfer to the instantiated contract.
    pub fn value(self, value: BalanceVariant) -> Self {
        let mut this = self;
        this.opts.value = value;
        this
    }

    /// Sets the maximum amount of gas to be used for this command.
    pub fn gas_limit(self, gas_limit: u64) -> Self {
        let mut this = self;
        this.opts.gas_limit = Some(gas_limit);
        this
    }

    /// Sets the maximum proof size for this instantiation.
    pub fn proof_size(self, proof_size: u64) -> Self {
        let mut this = self;
        this.opts.proof_size = Some(proof_size);
        this
    }

    /// Sets the salt used in the address derivation of the new contract.
    pub fn salt(self, salt: Bytes) -> Self {
        let mut this = self;
        this.opts.salt = Some(salt);
        this
    }

    /// Sets whether to export the call output in JSON format.
    pub fn output_json(self, output_json: bool) -> Self {
        let mut this = self;
        this.opts.output_json = output_json;
        this
    }
}

impl InstantiateCommandBuilder<state::ExtrinsicOptions> {
    /// Finishes construction of the instantiate command.
    pub async fn done(self) -> InstantiateExec {
        let instantiate_command = self.opts;
        instantiate_command.preprocess().await.unwrap()
    }
}

#[allow(clippy::new_ret_no_self)]
impl InstantiateCommand {
    /// Returns a clean builder for [`InstantiateCommand`].
    pub fn new() -> InstantiateCommandBuilder<Missing<state::ExtrinsicOptions>> {
        InstantiateCommandBuilder {
            opts: Self {
                constructor: String::from("new"),
                args: Vec::new(),
                extrinsic_opts: ExtrinsicOpts::default(),
                value: "0".parse().unwrap(),
                gas_limit: None,
                proof_size: None,
                salt: None,
                output_json: false,
            },
            marker: PhantomData,
        }
    }

    pub fn is_json(&self) -> bool {
        self.output_json
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
    pub async fn preprocess(&self) -> Result<InstantiateExec> {
        let artifacts = self.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;
        let data = transcoder.encode(&self.constructor, &self.args)?;
        let signer = self.extrinsic_opts.signer()?;
        let url = self.extrinsic_opts.url_to_string();
        let verbosity = self.extrinsic_opts.verbosity()?;
        let code = if let Some(code) = artifacts.code {
            Code::Upload(code.0)
        } else {
            let code_hash = artifacts.code_hash()?;
            Code::Existing(code_hash.into())
        };
        let salt = self.salt.clone().map(|s| s.0).unwrap_or_default();

        let client = OnlineClient::from_url(url.clone()).await?;

        let token_metadata = TokenMetadata::query(&client).await?;

        let args = InstantiateArgs {
            constructor: self.constructor.clone(),
            raw_args: self.args.clone(),
            value: self.value.denominate_balance(&token_metadata)?,
            gas_limit: self.gas_limit,
            proof_size: self.proof_size,
            storage_deposit_limit: self
                .extrinsic_opts
                .storage_deposit_limit
                .as_ref()
                .map(|bv| bv.denominate_balance(&token_metadata))
                .transpose()?,
            code,
            data,
            salt,
        };

        Ok(InstantiateExec {
            args,
            opts: self.extrinsic_opts.clone(),
            url,
            client,
            verbosity,
            signer,
            transcoder,
            output_json: self.output_json,
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
    pub fn storage_deposit_limit_compact(&self) -> Option<scale::Compact<Balance>> {
        self.storage_deposit_limit.map(Into::into)
    }

    pub fn code(&self) -> &Code {
        &self.code
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

pub struct InstantiateExec {
    opts: ExtrinsicOpts,
    args: InstantiateArgs,
    verbosity: Verbosity,
    url: String,
    client: Client,
    signer: Keypair,
    transcoder: ContractMessageTranscoder,
    output_json: bool,
}

impl InstantiateExec {
    /// Simulates the instantiation of a contract without executing it on the blockchain.
    ///
    /// This function performs a dry-run simulation of the contract instantiation process
    /// and returns an [`InstantiateDryRunResult`] object containing essential
    /// information, including the contract address, gas consumption, and storage
    /// deposit.
    ///
    /// It does not modify the state of the blockchain.
    pub async fn simulate_instantiation(
        &self,
    ) -> Result<InstantiateDryRunResult, ErrorVariant> {
        tracing::debug!("instantiate data {:?}", self.args.data);
        let result = self.instantiate_dry_run().await?;
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
                    gas_consumed: result.gas_consumed,
                    gas_required: result.gas_required,
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

    async fn instantiate_with_code(
        &self,
        code: Vec<u8>,
        gas_limit: Weight,
    ) -> Result<InstantiateExecResult, ErrorVariant> {
        let call = api::tx().contracts().instantiate_with_code(
            self.args.value,
            gas_limit.into(),
            self.args.storage_deposit_limit_compact(),
            code.to_vec(),
            self.args.data.clone(),
            self.args.salt.clone(),
        );

        let result = submit_extrinsic(&self.client, &call, &self.signer).await?;

        // The CodeStored event is only raised if the contract has not already been
        // uploaded.
        let code_hash = result
            .find_first::<api::contracts::events::CodeStored>()?
            .map(|code_stored| code_stored.code_hash);

        let instantiated = result
            .find_last::<api::contracts::events::Instantiated>()?
            .ok_or_else(|| anyhow!("Failed to find Instantiated event"))?;

        let token_metadata = TokenMetadata::query(&self.client).await?;
        Ok(InstantiateExecResult {
            result,
            code_hash,
            contract_address: instantiated.contract,
            token_metadata,
        })
    }

    async fn instantiate_with_code_hash(
        &self,
        code_hash: CodeHash,
        gas_limit: Weight,
    ) -> Result<InstantiateExecResult, ErrorVariant> {
        let call = api::tx().contracts().instantiate(
            self.args.value,
            gas_limit.into(),
            self.args.storage_deposit_limit_compact(),
            code_hash,
            self.args.data.clone(),
            self.args.salt.clone(),
        );

        let result = submit_extrinsic(&self.client, &call, &self.signer).await?;

        let instantiated = result
            .find_first::<api::contracts::events::Instantiated>()?
            .ok_or_else(|| anyhow!("Failed to find Instantiated event"))?;

        let token_metadata = TokenMetadata::query(&self.client).await?;
        Ok(InstantiateExecResult {
            result,
            code_hash: None,
            contract_address: instantiated.contract,
            token_metadata,
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
        gas_limit: Weight,
    ) -> Result<InstantiateExecResult, ErrorVariant> {
        match self.args.code.clone() {
            Code::Upload(code) => self.instantiate_with_code(code, gas_limit).await,
            Code::Existing(code_hash) => {
                self.instantiate_with_code_hash(code_hash, gas_limit).await
            }
        }
    }

    /// Displays the results of contract instantiation, including contract address,
    /// events, and optional code hash.
    pub async fn display_result(
        &self,
        instantiate_exec_result: InstantiateExecResult,
    ) -> Result<(), ErrorVariant> {
        let events = DisplayEvents::from_events(
            &instantiate_exec_result.result,
            Some(&self.transcoder),
            &self.client.metadata(),
        )?;
        let contract_address = instantiate_exec_result.contract_address.to_string();
        if self.output_json {
            let display_instantiate_result = InstantiateResult {
                code_hash: instantiate_exec_result
                    .code_hash
                    .map(|ch| format!("{ch:?}")),
                contract: Some(contract_address),
                events,
            };
            println!("{}", display_instantiate_result.to_json()?)
        } else {
            println!(
                "{}",
                events.display_events(
                    self.verbosity,
                    &instantiate_exec_result.token_metadata
                )?
            );
            if let Some(code_hash) = instantiate_exec_result.code_hash {
                name_value_println!("Code hash", format!("{code_hash:?}"));
            }
            name_value_println!("Contract", contract_address);
        };
        Ok(())
    }

    pub fn print_default_instantiate_preview(&self, gas_limit: Weight) {
        name_value_println!("Constructor", self.args.constructor, DEFAULT_KEY_COL_WIDTH);
        name_value_println!("Args", self.args.raw_args.join(" "), DEFAULT_KEY_COL_WIDTH);
        name_value_println!("Gas limit", gas_limit.to_string(), DEFAULT_KEY_COL_WIDTH);
    }

    /// Performs a dry run of the contract instantiation process without modifying the
    /// blockchain.
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
        state_call(&self.url, "ContractsApi_instantiate", &call_request).await
    }

    /// Estimates the gas required for the contract instantiation process without
    /// modifying the blockchain.
    ///
    /// This function provides a gas estimation for contract instantiation, considering
    /// the user-specified values or using estimates based on a dry run.
    ///
    /// Returns the estimated gas weight of type [`Weight`] for contract instantiation, or
    /// an error.
    pub async fn estimate_gas(&self, print_to_terminal: bool) -> Result<Weight> {
        if self.opts.skip_dry_run {
            return match (self.args.gas_limit, self.args.proof_size) {
                (Some(ref_time), Some(proof_size)) => Ok(Weight::from_parts(ref_time, proof_size)),
                _ => {
                    Err(anyhow!(
                        "Weight args `--gas` and `--proof-size` required if `--skip-dry-run` specified"
                    ))
                }
            };
        }
        if !self.output_json && print_to_terminal {
            super::print_dry_running_status(&self.args.constructor);
        }
        let instantiate_result = self.instantiate_dry_run().await?;
        match instantiate_result.result {
            Ok(_) => {
                if !self.output_json && print_to_terminal {
                    super::print_gas_required_success(instantiate_result.gas_required);
                }
                // use user specified values where provided, otherwise use the estimates
                let ref_time = self
                    .args
                    .gas_limit
                    .unwrap_or_else(|| instantiate_result.gas_required.ref_time());
                let proof_size = self
                    .args
                    .proof_size
                    .unwrap_or_else(|| instantiate_result.gas_required.proof_size());
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
                            &instantiate_result,
                        )?;
                    }
                    Err(anyhow!("Pre-submission dry-run failed. Use --skip-dry-run to skip this step."))
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

    /// Returns the verbosity level.
    pub fn verbosity(&self) -> Verbosity {
        self.verbosity
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

    /// Returns whether to export the call output in JSON format.
    pub fn output_json(&self) -> bool {
        self.output_json
    }
}

pub struct InstantiateExecResult {
    pub result: ExtrinsicEvents<DefaultConfig>,
    pub code_hash: Option<CodeHash>,
    pub contract_address: subxt::utils::AccountId32,
    pub token_metadata: TokenMetadata,
}

/// Result of a successful contract instantiation for displaying.
#[derive(serde::Serialize)]
pub struct InstantiateResult {
    /// Instantiated contract hash
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
    /// Instantiated code hash
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_hash: Option<String>,
    /// The events emitted from the instantiate extrinsic invocation.
    pub events: DisplayEvents,
}

impl InstantiateResult {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
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

    pub fn print(&self) {
        name_value_println!("Result", format!("{}", self.result), DEFAULT_KEY_COL_WIDTH);
        name_value_println!(
            "Reverted",
            format!("{:?}", self.reverted),
            DEFAULT_KEY_COL_WIDTH
        );
        name_value_println!("Contract", self.contract, DEFAULT_KEY_COL_WIDTH);
        name_value_println!(
            "Gas consumed",
            self.gas_consumed.to_string(),
            DEFAULT_KEY_COL_WIDTH
        );
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

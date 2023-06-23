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
    state_call,
    submit_extrinsic,
    BalanceVariant,
    Client,
    ContractMessageTranscoder,
    DefaultConfig,
    ExtrinsicOpts,
    PairSigner,
    StorageDeposit,
    MAX_KEY_COL_WIDTH,
};
use crate::{
    cmd::{
        extrinsics::{
            display_contract_exec_result_debug,
            display_dry_run_result_warning,
            events::DisplayEvents,
            ErrorVariant,
            TokenMetadata,
        },
        runtime_api::api,
        Balance,
        CodeHash,
    },
    DEFAULT_KEY_COL_WIDTH,
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

use pallet_contracts_primitives::ContractInstantiateResult;

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

impl InstantiateCommand {
    pub fn is_json(&self) -> bool {
        self.output_json
    }
    /// Instantiate a contract stored at the supplied code hash.
    /// Returns the account id of the instantiated contract if successful.
    ///
    /// Creates an extrinsic with the `Contracts::instantiate` Call, submits via RPC, then
    /// waits for the `ContractsEvent::Instantiated` event.
    pub fn run(&self) -> Result<(), ErrorVariant> {
        let artifacts = self.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;
        let data = transcoder.encode(&self.constructor, &self.args)?;
        let signer = super::pair_signer(self.extrinsic_opts.signer()?);
        let url = self.extrinsic_opts.url_to_string();
        let verbosity = self.extrinsic_opts.verbosity()?;
        let code = if let Some(code) = artifacts.code {
            Code::Upload(code.0)
        } else {
            let code_hash = artifacts.code_hash()?;
            Code::Existing(code_hash.into())
        };
        let salt = self.salt.clone().map(|s| s.0).unwrap_or_default();

        async_std::task::block_on(async move {
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

            let exec = Exec {
                args,
                opts: self.extrinsic_opts.clone(),
                url,
                client,
                verbosity,
                signer,
                transcoder,
                output_json: self.output_json,
            };

            exec.exec(self.extrinsic_opts.execute).await
        })
    }
}

struct InstantiateArgs {
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
    fn storage_deposit_limit_compact(&self) -> Option<scale::Compact<Balance>> {
        self.storage_deposit_limit.map(Into::into)
    }
}

pub struct Exec {
    opts: ExtrinsicOpts,
    args: InstantiateArgs,
    verbosity: Verbosity,
    url: String,
    client: Client,
    signer: PairSigner,
    transcoder: ContractMessageTranscoder,
    output_json: bool,
}

impl Exec {
    async fn exec(&self, execute: bool) -> Result<(), ErrorVariant> {
        tracing::debug!("instantiate data {:?}", self.args.data);
        if !execute {
            let result = self.instantiate_dry_run().await?;
            match result.result {
                Ok(ref ret_val) => {
                    let value = self
                        .transcoder
                        .decode_constructor_return(
                            &self.args.constructor,
                            &mut &ret_val.result.data[..],
                        )
                        .context(format!(
                            "Failed to decode return value {:?}",
                            &ret_val
                        ))?;
                    let dry_run_result = InstantiateDryRunResult {
                        result: value,
                        contract: ret_val.account_id.to_string(),
                        reverted: ret_val.result.did_revert(),
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
                        display_dry_run_result_warning("instantiate");
                    }
                }
                Err(ref err) => {
                    let metadata = self.client.metadata();
                    let object = ErrorVariant::from_dispatch_error(err, &metadata)?;
                    if self.output_json {
                        return Err(object)
                    } else {
                        name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                        display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(&result)?;
                    }
                }
            }
        } else {
            let gas_limit = self.pre_submit_dry_run_gas_estimate().await?;
            match self.args.code.clone() {
                Code::Upload(code) => {
                    self.instantiate_with_code(code, gas_limit).await?;
                }
                Code::Existing(code_hash) => {
                    self.instantiate(code_hash, gas_limit).await?;
                }
            }
        }
        Ok(())
    }

    async fn instantiate_with_code(
        &self,
        code: Vec<u8>,
        gas_limit: Weight,
    ) -> Result<(), ErrorVariant> {
        if !self.opts.skip_confirm {
            prompt_confirm_tx(|| self.print_default_instantiate_preview(gas_limit))?;
        }

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
        self.display_result(&result, code_hash, instantiated.contract, &token_metadata)
            .await
    }

    async fn instantiate(
        &self,
        code_hash: CodeHash,
        gas_limit: Weight,
    ) -> Result<(), ErrorVariant> {
        if !self.opts.skip_confirm {
            prompt_confirm_tx(|| {
                self.print_default_instantiate_preview(gas_limit);
                name_value_println!(
                    "Code hash",
                    format!("{code_hash:?}"),
                    DEFAULT_KEY_COL_WIDTH
                );
            })?;
        }

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
        self.display_result(&result, None, instantiated.contract, &token_metadata)
            .await
    }

    async fn display_result(
        &self,
        result: &ExtrinsicEvents<DefaultConfig>,
        code_hash: Option<CodeHash>,
        contract_address: subxt::utils::AccountId32,
        token_metadata: &TokenMetadata,
    ) -> Result<(), ErrorVariant> {
        let events = DisplayEvents::from_events(
            result,
            Some(&self.transcoder),
            &self.client.metadata(),
        )?;
        let contract_address = contract_address.to_string();

        if self.output_json {
            let display_instantiate_result = InstantiateResult {
                code_hash: code_hash.map(|ch| format!("{ch:?}")),
                contract: Some(contract_address),
                events,
            };
            println!("{}", display_instantiate_result.to_json()?)
        } else {
            println!("{}", events.display_events(self.verbosity, token_metadata)?);
            if let Some(code_hash) = code_hash {
                name_value_println!("Code hash", format!("{code_hash:?}"));
            }
            name_value_println!("Contract", contract_address);
        };
        Ok(())
    }

    fn print_default_instantiate_preview(&self, gas_limit: Weight) {
        name_value_println!("Constructor", self.args.constructor, DEFAULT_KEY_COL_WIDTH);
        name_value_println!("Args", self.args.raw_args.join(" "), DEFAULT_KEY_COL_WIDTH);
        name_value_println!("Gas limit", gas_limit.to_string(), DEFAULT_KEY_COL_WIDTH);
    }

    async fn instantiate_dry_run(
        &self,
    ) -> Result<
        ContractInstantiateResult<<DefaultConfig as Config>::AccountId, Balance, ()>,
    > {
        let storage_deposit_limit = self.args.storage_deposit_limit;
        let call_request = InstantiateRequest {
            origin: self.signer.account_id().clone(),
            value: self.args.value,
            gas_limit: None,
            storage_deposit_limit,
            code: self.args.code.clone(),
            data: self.args.data.clone(),
            salt: self.args.salt.clone(),
        };
        state_call(&self.url, "ContractsApi_instantiate", &call_request).await
    }

    /// Dry run the instantiation before tx submission. Returns the gas required estimate.
    async fn pre_submit_dry_run_gas_estimate(&self) -> Result<Weight> {
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
        if !self.output_json {
            super::print_dry_running_status(&self.args.constructor);
        }
        let instantiate_result = self.instantiate_dry_run().await?;
        match instantiate_result.result {
            Ok(_) => {
                if !self.output_json {
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
                    name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                    display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(
                        &instantiate_result,
                    )?;
                    Err(anyhow!("Pre-submission dry-run failed. Use --skip-dry-run to skip this step."))
                }
            }
        }
    }
}

/// Result of a successful contract instantiation for displaying.
#[derive(serde::Serialize)]
pub struct InstantiateResult {
    /// Instantiated contract hash
    #[serde(skip_serializing_if = "Option::is_none")]
    contract: Option<String>,
    /// Instantiated code hash
    #[serde(skip_serializing_if = "Option::is_none")]
    code_hash: Option<String>,
    /// The events emitted from the instantiate extrinsic invocation.
    events: DisplayEvents,
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
enum Code {
    /// A Wasm module as raw bytes.
    Upload(Vec<u8>),
    /// The code hash of an on-chain Wasm blob.
    Existing(<DefaultConfig as Config>::Hash),
}

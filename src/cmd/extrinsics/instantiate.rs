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
    error_details,
    events::parse_events,
    parse_balance,
    prompt_confirm_tx,
    runtime_api::api,
    state_call,
    submit_extrinsic,
    Balance,
    Client,
    CodeHash,
    ContractAccount,
    ContractMessageTranscoder,
    CrateMetadata,
    DefaultConfig,
    ExtrinsicOpts,
    PairSigner,
    MAX_KEY_COL_WIDTH,
};
use crate::{
    name_value_println,
    util::decode_hex,
    Verbosity,
    DEFAULT_KEY_COL_WIDTH,
};
use anyhow::{
    anyhow,
    Context,
    Result,
};

use pallet_contracts_primitives::ContractInstantiateResult;

use scale::Encode;
use sp_core::{
    crypto::Ss58Codec,
    Bytes,
};
use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};
use subxt::{
    Config,
    OnlineClient,
};

#[derive(Debug, clap::Args)]
pub struct InstantiateCommand {
    /// Path to Wasm contract code, defaults to `./target/ink/<name>.wasm`.
    /// Use to instantiate contracts which have not yet been uploaded.
    /// If the contract has already been uploaded use `--code-hash` instead.
    #[clap(parse(from_os_str))]
    wasm_path: Option<PathBuf>,
    /// The hash of the smart contract code already uploaded to the chain.
    /// If the contract has not already been uploaded use `--wasm-path` or run the `upload` command
    /// first.
    #[clap(long, parse(try_from_str = parse_code_hash))]
    code_hash: Option<<DefaultConfig as Config>::Hash>,
    /// The name of the contract constructor to call
    #[clap(name = "constructor", long, default_value = "new")]
    constructor: String,
    /// The constructor arguments, encoded as strings
    #[clap(long, multiple_values = true)]
    args: Vec<String>,
    #[clap(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Transfers an initial balance to the instantiated contract
    #[clap(name = "value", long, default_value = "0", parse(try_from_str = parse_balance))]
    value: Balance,
    /// Maximum amount of gas to be used for this command.
    /// If not specified will perform a dry-run to estimate the gas consumed for the instantiation.
    #[clap(name = "gas", long)]
    gas_limit: Option<u64>,
    /// A salt used in the address derivation of the new contract. Use to create multiple instances
    /// of the same contract code from the same account.
    #[clap(long, parse(try_from_str = parse_hex_bytes))]
    salt: Option<Bytes>,
}

/// Parse a hex encoded 32 byte hash. Returns error if not exactly 32 bytes.
fn parse_code_hash(input: &str) -> Result<<DefaultConfig as Config>::Hash> {
    let bytes = decode_hex(input)?;
    if bytes.len() != 32 {
        anyhow::bail!("Code hash should be 32 bytes in length")
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr.into())
}

/// Parse hex encoded bytes.
fn parse_hex_bytes(input: &str) -> Result<Bytes> {
    let bytes = decode_hex(input)?;
    Ok(bytes.into())
}

impl InstantiateCommand {
    /// Instantiate a contract stored at the supplied code hash.
    /// Returns the account id of the instantiated contract if successful.
    ///
    /// Creates an extrinsic with the `Contracts::instantiate` Call, submits via RPC, then waits for
    /// the `ContractsEvent::Instantiated` event.
    pub fn run(&self) -> Result<()> {
        let crate_metadata = CrateMetadata::from_manifest_path(
            self.extrinsic_opts.manifest_path.as_ref(),
        )?;
        let transcoder = ContractMessageTranscoder::load(crate_metadata.metadata_path())?;
        let data = transcoder.encode(&self.constructor, &self.args)?;
        let signer = super::pair_signer(self.extrinsic_opts.signer()?);
        let url = self.extrinsic_opts.url_to_string();
        let verbosity = self.extrinsic_opts.verbosity()?;

        fn load_code(wasm_path: &Path) -> Result<Code> {
            tracing::debug!("Contract code path: {}", wasm_path.display());
            let code = fs::read(&wasm_path)
                .context(format!("Failed to read from {}", wasm_path.display()))?;
            Ok(Code::Upload(code))
        }

        let code = match (self.wasm_path.as_ref(), self.code_hash.as_ref()) {
            (Some(_), Some(_)) => {
                Err(anyhow!(
                    "Specify either `--wasm-path` or `--code-hash` but not both"
                ))
            }
            (Some(wasm_path), None) => load_code(wasm_path),
            (None, None) => {
                // default to the target contract wasm in the current project,
                // inferred via the crate metadata.
                load_code(&crate_metadata.dest_wasm)
            }
            (None, Some(code_hash)) => Ok(Code::Existing(*code_hash)),
        }?;
        let salt = self.salt.clone().map(|s| s.0).unwrap_or_default();

        let args = InstantiateArgs {
            constructor: self.constructor.clone(),
            raw_args: self.args.clone(),
            value: self.value,
            gas_limit: self.gas_limit,
            storage_deposit_limit: self.extrinsic_opts.storage_deposit_limit,
            data,
            salt,
        };

        async_std::task::block_on(async move {
            let client = OnlineClient::from_url(url.clone()).await?;

            let exec = Exec {
                args,
                opts: self.extrinsic_opts.clone(),
                url,
                client,
                verbosity,
                signer,
                transcoder,
            };

            exec.exec(code, self.extrinsic_opts.dry_run).await
        })
    }
}

struct InstantiateArgs {
    constructor: String,
    raw_args: Vec<String>,
    value: Balance,
    gas_limit: Option<u64>,
    storage_deposit_limit: Option<Balance>,
    data: Vec<u8>,
    salt: Vec<u8>,
}

pub struct Exec {
    opts: ExtrinsicOpts,
    args: InstantiateArgs,
    verbosity: Verbosity,
    url: String,
    client: Client,
    signer: PairSigner,
    transcoder: ContractMessageTranscoder,
}

impl Exec {
    async fn exec(&self, code: Code, dry_run: bool) -> Result<()> {
        tracing::debug!("instantiate data {:?}", self.args.data);
        if dry_run {
            let result = self.instantiate_dry_run(code).await?;
            match result.result {
                Ok(ref ret_val) => {
                    name_value_println!(
                        "Result",
                        String::from("Success!"),
                        DEFAULT_KEY_COL_WIDTH
                    );
                    name_value_println!(
                        "Contract",
                        ret_val.account_id.to_ss58check(),
                        DEFAULT_KEY_COL_WIDTH
                    );
                    name_value_println!(
                        "Reverted",
                        format!("{:?}", ret_val.result.did_revert()),
                        DEFAULT_KEY_COL_WIDTH
                    );
                    name_value_println!(
                        "Data",
                        format!("{:?}", ret_val.result.data),
                        DEFAULT_KEY_COL_WIDTH
                    );
                    display_contract_exec_result::<_, DEFAULT_KEY_COL_WIDTH>(&result)
                }
                Err(ref err) => {
                    let metadata = self.client.metadata();
                    let err = error_details(err, &metadata)?;
                    name_value_println!("Result", err, MAX_KEY_COL_WIDTH);
                    display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(&result)
                }
            }
        } else {
            match code {
                Code::Upload(code) => {
                    let (code_hash, contract_account) =
                        self.instantiate_with_code(code).await?;
                    if let Some(code_hash) = code_hash {
                        name_value_println!("Code hash", format!("{:?}", code_hash));
                    }
                    name_value_println!("Contract", contract_account.to_ss58check());
                }
                Code::Existing(code_hash) => {
                    let contract_account = self.instantiate(code_hash).await?;
                    name_value_println!("Contract", contract_account.to_ss58check());
                }
            }
            Ok(())
        }
    }

    async fn instantiate_with_code(
        &self,
        code: Vec<u8>,
    ) -> Result<(Option<CodeHash>, ContractAccount)> {
        let gas_limit = self
            .pre_submit_dry_run_gas_estimate(Code::Upload(code.clone()))
            .await?;

        if !self.opts.skip_confirm {
            prompt_confirm_tx(|| self.print_default_instantiate_preview(gas_limit))?;
        }

        let call = api::tx().contracts().instantiate_with_code(
            self.args.value,
            gas_limit,
            self.args.storage_deposit_limit,
            code.to_vec(),
            self.args.data.clone(),
            self.args.salt.clone(),
        );

        let result = submit_extrinsic(&self.client, &call, &self.signer).await?;

        let call_result = parse_events(
            &result,
            &self.transcoder,
            &self.client.metadata(),
            Default::default(),
        )?;

        let display = call_result.display(&self.verbosity);
        println!("{}", display);

        // The CodeStored event is only raised if the contract has not already been uploaded.
        let code_hash = result
            .find_first::<api::contracts::events::CodeStored>()?
            .map(|code_stored| code_stored.code_hash);

        let instantiated = result
            .find_first::<api::contracts::events::Instantiated>()?
            .ok_or_else(|| anyhow!("Failed to find Instantiated event"))?;

        Ok((code_hash, instantiated.contract))
    }

    async fn instantiate(&self, code_hash: CodeHash) -> Result<ContractAccount> {
        let gas_limit = self
            .pre_submit_dry_run_gas_estimate(Code::Existing(code_hash))
            .await?;

        if !self.opts.skip_confirm {
            prompt_confirm_tx(|| {
                self.print_default_instantiate_preview(gas_limit);
                name_value_println!(
                    "Code hash",
                    format!("{:?}", code_hash),
                    DEFAULT_KEY_COL_WIDTH
                );
            })?;
        }

        let call = api::tx().contracts().instantiate(
            self.args.value,
            gas_limit,
            self.args.storage_deposit_limit,
            code_hash,
            self.args.data.clone(),
            self.args.salt.clone(),
        );

        let result = submit_extrinsic(&self.client, &call, &self.signer).await?;

        let call_result = parse_events(
            &result,
            &self.transcoder,
            &self.client.metadata(),
            Default::default(),
        )?;

        let display = call_result.display(&self.verbosity);
        println!("{}", display);

        let instantiated = result
            .find_first::<api::contracts::events::Instantiated>()?
            .ok_or_else(|| anyhow!("Failed to find Instantiated event"))?;

        Ok(instantiated.contract)
    }

    fn print_default_instantiate_preview(&self, gas_limit: u64) {
        name_value_println!("Constructor", self.args.constructor, DEFAULT_KEY_COL_WIDTH);
        name_value_println!("Args", self.args.raw_args.join(" "), DEFAULT_KEY_COL_WIDTH);
        name_value_println!("Gas limit", gas_limit.to_string(), DEFAULT_KEY_COL_WIDTH);
    }

    async fn instantiate_dry_run(
        &self,
        code: Code,
    ) -> Result<ContractInstantiateResult<<DefaultConfig as Config>::AccountId, Balance>>
    {
        let gas_limit = *self.args.gas_limit.as_ref().unwrap_or(&5_000_000_000_000);
        let storage_deposit_limit = self.args.storage_deposit_limit;
        let call_request = InstantiateRequest {
            origin: self.signer.account_id().clone(),
            value: self.args.value,
            gas_limit,
            storage_deposit_limit,
            code,
            data: self.args.data.clone(),
            salt: self.args.salt.clone(),
        };
        state_call(&self.url, "ContractsApi_instantiate", &call_request).await
    }

    /// Dry run the instantiation before tx submission. Returns the gas required estimate.
    async fn pre_submit_dry_run_gas_estimate(&self, code: Code) -> Result<u64> {
        if self.opts.skip_dry_run {
            return match self.args.gas_limit {
                Some(gas) => Ok(gas),
                None => {
                    Err(anyhow!(
                    "Gas limit `--gas` argument required if `--skip-dry-run` specified"
                ))
                }
            }
        }
        super::print_dry_running_status(&self.args.constructor);
        let instantiate_result = self.instantiate_dry_run(code).await?;
        match instantiate_result.result {
            Ok(_) => {
                super::print_gas_required_success(instantiate_result.gas_required);
                let gas_limit = self
                    .args
                    .gas_limit
                    .unwrap_or(instantiate_result.gas_required);
                Ok(gas_limit)
            }
            Err(ref err) => {
                let err = error_details(err, &self.client.metadata())?;
                name_value_println!("Result", err, MAX_KEY_COL_WIDTH);
                display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(
                    &instantiate_result,
                )?;
                Err(anyhow!("Pre-submission dry-run failed. Use --skip-dry-run to skip this step."))
            }
        }
    }
}

/// A struct that encodes RPC parameters required to instantiate a new smart contract.
#[derive(Encode)]
struct InstantiateRequest {
    origin: <DefaultConfig as Config>::AccountId,
    value: Balance,
    gas_limit: u64,
    storage_deposit_limit: Option<Balance>,
    code: Code,
    data: Vec<u8>,
    salt: Vec<u8>,
}

/// Reference to an existing code hash or a new Wasm module.
#[derive(Encode)]
enum Code {
    /// A Wasm module as raw bytes.
    Upload(Vec<u8>),
    /// The code hash of an on-chain Wasm blob.
    Existing(<DefaultConfig as Config>::Hash),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_code_hash_works() {
        // with 0x prefix
        assert!(parse_code_hash(
            "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
        )
        .is_ok());
        // without 0x prefix
        assert!(parse_code_hash(
            "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
        )
        .is_ok())
    }
}

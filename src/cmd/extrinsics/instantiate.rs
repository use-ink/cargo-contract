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
    display_contract_exec_result, display_events, parse_balance, runtime_api::api,
    wait_for_success_and_handle_error, Balance, CodeHash, ContractAccount,
    ContractMessageTranscoder, ExtrinsicOpts, PairSigner, RuntimeApi,
    EXEC_RESULT_MAX_KEY_COL_WIDTH,
};
use crate::{name_value_println, util::decode_hex, Verbosity};
use anyhow::{anyhow, Context, Result};
use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};
use serde::Serialize;
use sp_core::{crypto::Ss58Codec, Bytes};
use std::{
    fs,
    path::{Path, PathBuf},
};
use structopt::StructOpt;
use subxt::{rpc::NumberOrHex, ClientBuilder, Config, DefaultConfig, Signer};

type ContractInstantiateResult =
    pallet_contracts_primitives::ContractInstantiateResult<ContractAccount, Balance>;

#[derive(Debug, StructOpt)]
pub struct InstantiateCommand {
    /// Path to Wasm contract code, defaults to `./target/ink/<name>.wasm`.
    /// Use to instantiate contracts which have not yet been uploaded.
    /// If the contract has already been uploaded use `--code-hash` instead.
    #[structopt(parse(from_os_str))]
    wasm_path: Option<PathBuf>,
    /// The hash of the smart contract code already uploaded to the chain.
    /// If the contract has not already been uploaded use `--wasm-path` or run the `upload` command
    /// first.
    #[structopt(long, parse(try_from_str = parse_code_hash))]
    code_hash: Option<<DefaultConfig as Config>::Hash>,
    /// The name of the contract constructor to call
    #[structopt(name = "constructor", long, default_value = "new")]
    constructor: String,
    /// The constructor arguments, encoded as strings
    #[structopt(long)]
    args: Vec<String>,
    #[structopt(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Transfers an initial balance to the instantiated contract
    #[structopt(name = "value", long, default_value = "0", parse(try_from_str = parse_balance))]
    value: Balance,
    /// Maximum amount of gas to be used for this command
    #[structopt(name = "gas", long, default_value = "50000000000")]
    gas_limit: u64,
    /// A salt used in the address derivation of the new contract. Use to create multiple instances
    /// of the same contract code from the same account.
    #[structopt(long, parse(try_from_str = parse_hex_bytes))]
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
        let (crate_metadata, contract_metadata) =
            super::load_metadata(self.extrinsic_opts.manifest_path.as_ref())?;
        let transcoder = ContractMessageTranscoder::new(&contract_metadata);
        let data = transcoder.encode(&self.constructor, &self.args)?;
        let signer = super::pair_signer(self.extrinsic_opts.signer()?);
        let url = self.extrinsic_opts.url.clone();
        let verbosity = self.extrinsic_opts.verbosity()?;

        fn load_code(wasm_path: &Path) -> Result<Code> {
            log::info!("Contract code path: {}", wasm_path.display());
            let code = fs::read(&wasm_path)
                .context(format!("Failed to read from {}", wasm_path.display()))?;
            Ok(Code::Upload(code.into()))
        }

        let code = match (self.wasm_path.as_ref(), self.code_hash.as_ref()) {
            (Some(_), Some(_)) => Err(anyhow!(
                "Specify either `--wasm-path` or `--code-hash` but not both"
            )),
            (Some(wasm_path), None) => load_code(wasm_path),
            (None, None) => {
                // default to the target contract wasm in the current project,
                // inferred via the crate metadata.
                load_code(&crate_metadata.dest_wasm)
            }
            (None, Some(code_hash)) => Ok(Code::Existing(*code_hash)),
        }?;
        let salt = self.salt.clone().unwrap_or_else(|| Bytes(Vec::new()));

        let args = InstantiateArgs {
            value: self.value,
            gas_limit: self.gas_limit,
            storage_deposit_limit: self.extrinsic_opts.storage_deposit_limit,
            data,
            salt,
        };

        let exec = Exec {
            args,
            url,
            verbosity,
            signer,
            transcoder,
        };

        async_std::task::block_on(async move { exec.exec(code, self.extrinsic_opts.dry_run).await })
    }
}

struct InstantiateArgs {
    value: super::Balance,
    gas_limit: u64,
    storage_deposit_limit: Option<Balance>,
    data: Vec<u8>,
    salt: Bytes,
}

pub struct Exec<'a> {
    args: InstantiateArgs,
    verbosity: Verbosity,
    url: url::Url,
    signer: PairSigner,
    transcoder: ContractMessageTranscoder<'a>,
}

impl<'a> Exec<'a> {
    async fn subxt_api(&self) -> Result<RuntimeApi> {
        let api = ClientBuilder::new()
            .set_url(self.url.to_string())
            .build()
            .await?
            .to_runtime_api::<RuntimeApi>();
        Ok(api)
    }

    async fn exec(&self, code: Code, dry_run: bool) -> Result<()> {
        if dry_run {
            let result = self.instantiate_dry_run(code).await?;
            match result.result {
                Ok(ref ret_val) => {
                    name_value_println!(
                        "Result",
                        String::from("Success!"),
                        EXEC_RESULT_MAX_KEY_COL_WIDTH
                    );
                    name_value_println!(
                        "Contract",
                        ret_val.account_id.to_ss58check(),
                        EXEC_RESULT_MAX_KEY_COL_WIDTH
                    );
                    name_value_println!(
                        "Reverted",
                        format!("{:?}", ret_val.result.did_revert()),
                        EXEC_RESULT_MAX_KEY_COL_WIDTH
                    );
                    name_value_println!(
                        "Data",
                        format!("{:?}", ret_val.result.data),
                        EXEC_RESULT_MAX_KEY_COL_WIDTH
                    );
                }
                Err(err) => {
                    name_value_println!(
                        "Result",
                        format!("Error: {:?}", err),
                        EXEC_RESULT_MAX_KEY_COL_WIDTH
                    );
                }
            }
            display_contract_exec_result(&result)?;
            return Ok(());
        }

        match code {
            Code::Upload(code) => {
                let (code_hash, contract_account) = self.instantiate_with_code(code).await?;
                name_value_println!("Code hash", format!("{:?}", code_hash));
                name_value_println!("Contract", contract_account.to_ss58check());
            }
            Code::Existing(code_hash) => {
                let contract_account = self.instantiate(code_hash).await?;
                name_value_println!("Contract", contract_account.to_ss58check());
            }
        }
        Ok(())
    }

    async fn instantiate_with_code(&self, code: Bytes) -> Result<(CodeHash, ContractAccount)> {
        let api = self.subxt_api().await?;
        let tx_progress = api
            .tx()
            .contracts()
            .instantiate_with_code(
                self.args.value,
                self.args.gas_limit,
                self.args.storage_deposit_limit,
                code.to_vec(),
                self.args.data.clone(),
                self.args.salt.0.clone(),
            )
            .sign_and_submit_then_watch(&self.signer)
            .await?;

        let result = wait_for_success_and_handle_error(tx_progress).await?;

        let metadata = api.client.metadata();

        display_events(&result, &self.transcoder, metadata, &self.verbosity)?;

        let code_stored = result
            .find_first::<api::contracts::events::CodeStored>()?
            .ok_or(anyhow!("Failed to find CodeStored event"))?;
        let instantiated = result
            .find_first::<api::contracts::events::Instantiated>()?
            .ok_or(anyhow!("Failed to find Instantiated event"))?;

        Ok((code_stored.code_hash, instantiated.contract))
    }

    async fn instantiate(&self, code_hash: CodeHash) -> Result<ContractAccount> {
        let api = self.subxt_api().await?;
        let tx_progress = api
            .tx()
            .contracts()
            .instantiate(
                self.args.value,
                self.args.gas_limit,
                self.args.storage_deposit_limit,
                code_hash,
                self.args.data.clone(),
                self.args.salt.0.clone(),
            )
            .sign_and_submit_then_watch(&self.signer)
            .await?;

        let result = wait_for_success_and_handle_error(tx_progress).await?;

        let metadata = api.client.metadata();
        display_events(&result, &self.transcoder, metadata, &self.verbosity)?;

        let instantiated = result
            .find_first::<api::contracts::events::Instantiated>()?
            .ok_or(anyhow!("Failed to find Instantiated event"))?;

        Ok(instantiated.contract)
    }

    async fn instantiate_dry_run(&self, code: Code) -> Result<ContractInstantiateResult> {
        let url = self.url.to_string();
        let cli = WsClientBuilder::default().build(&url).await?;
        let storage_deposit_limit = self
            .args
            .storage_deposit_limit
            .as_ref()
            .map(|limit| NumberOrHex::Hex((*limit).into()));
        let call_request = InstantiateRequest {
            origin: self.signer.account_id().clone(),
            value: NumberOrHex::Hex(self.args.value.into()),
            gas_limit: NumberOrHex::Number(self.args.gas_limit),
            storage_deposit_limit,
            code,
            data: self.args.data.clone().into(),
            salt: self.args.salt.clone(),
        };
        let params = rpc_params![call_request];
        let result: ContractInstantiateResult =
            cli.request("contracts_instantiate", params).await?;
        Ok(result)
    }
}

/// A struct that encodes RPC parameters required to instantiate a new smart contract.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstantiateRequest {
    origin: <DefaultConfig as Config>::AccountId,
    value: NumberOrHex,
    gas_limit: NumberOrHex,
    storage_deposit_limit: Option<NumberOrHex>,
    code: Code,
    data: Bytes,
    salt: Bytes,
}

/// Reference to an existing code hash or a new Wasm module.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum Code {
    /// A Wasm module as raw bytes.
    Upload(Bytes),
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
        assert!(
            parse_code_hash("d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d")
                .is_ok()
        )
    }
}

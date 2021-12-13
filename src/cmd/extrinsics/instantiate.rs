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
    display_events,
    runtime_api::api::{self, DefaultConfig},
    Balance, ContractMessageTranscoder,
};
use crate::{util::decode_hex, ExtrinsicOpts, Verbosity};
use anyhow::{Context, Result};
use jsonrpsee::{
    types::{to_json_value, traits::Client as _},
    ws_client::WsClientBuilder,
};
use serde::Serialize;
use sp_core::Bytes;
use std::{fs, path::PathBuf};
use structopt::StructOpt;
use subxt::{rpc::NumberOrHex, ClientBuilder, Config, Signer};

type CodeHash = <DefaultConfig as Config>::Hash;
type ContractAccount = <DefaultConfig as Config>::AccountId;
type ContractInstantiateResult =
    pallet_contracts_primitives::ContractInstantiateResult<ContractAccount, Balance>;

#[derive(Debug, StructOpt)]
pub struct InstantiateCommand {
    /// The name of the contract constructor to call
    #[structopt(name = "constructor", long, default_value = "new")]
    pub(super) constructor: String,
    /// The constructor parameters, encoded as strings
    #[structopt(name = "params", long, default_value = "new")]
    pub(super) params: Vec<String>,
    #[structopt(flatten)]
    pub(super) extrinsic_opts: ExtrinsicOpts,
    /// Transfers an initial balance to the instantiated contract
    #[structopt(name = "endowment", long, default_value = "0")]
    pub(super) value: super::Balance,
    /// Maximum amount of gas to be used for this command
    #[structopt(name = "gas", long, default_value = "50000000000")]
    pub(super) gas_limit: u64,
    /// The maximum amount of balance that can be charged from the caller to pay for the storage
    /// consumed.
    #[structopt(long)]
    pub(super) storage_deposit_limit: Option<Balance>,
    /// Path to wasm contract code, defaults to `./target/ink/<name>.wasm`.
    /// Use to instantiate contracts which have not yet been uploaded.
    /// If the contract has already been uploaded use `--code_hash` instead.
    #[structopt(parse(from_os_str))]
    pub(super) wasm_path: Option<PathBuf>,
    // todo: [AJ] add salt
    /// The hash of the smart contract code already uploaded to the chain.
    /// If the contract has not already been uploaded use `--wasm-path` or run the `upload` command
    /// first.
    #[structopt(long, parse(try_from_str = parse_code_hash))]
    code_hash: Option<<DefaultConfig as Config>::Hash>,
    /// Dry-run instantiate via RPC, instead of as an extrinsic.
    /// The contract will not be instantiated.
    #[structopt(long)]
    dry_run: bool,
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

impl InstantiateCommand {
    /// Instantiate a contract stored at the supplied code hash.
    /// Returns the account id of the instantiated contract if successful.
    ///
    /// Creates an extrinsic with the `Contracts::instantiate` Call, submits via RPC, then waits for
    /// the `ContractsEvent::Instantiated` event.
    pub fn run(&self) -> Result<()> {
        let metadata = super::load_metadata()?;
        let transcoder = ContractMessageTranscoder::new(&metadata);
        let data = transcoder.encode(&self.constructor, &self.params)?;
        let signer = super::pair_signer(self.extrinsic_opts.signer()?);
        let url = self.extrinsic_opts.url.clone();
        let verbosity = self.extrinsic_opts.verbosity()?;

        let code = match (self.wasm_path.as_ref(), self.code_hash.as_ref()) {
            (Some(wasm_path), None) => {
                log::info!("Contract code path: {}", wasm_path.display());
                let code = fs::read(&wasm_path)
                    .context(format!("Failed to read from {}", wasm_path.display()))?;
                Ok(Code::Upload(code.into()))
            }
            (None, Some(code_hash)) => Ok(Code::Existing(*code_hash)),
            (Some(_), Some(_)) => Err(anyhow::anyhow!(
                "Specify either `--wasm-path` or `--code-hash` but not both"
            )),
            (None, None) => Err(anyhow::anyhow!(
                "Specify one of `--wasm-path` or `--code-hash`"
            )),
        }?;

        let args = InstantiateArgs {
            value: self.value,
            gas_limit: self.gas_limit,
            storage_deposit_limit: self.storage_deposit_limit,
            data,
            // todo: [AJ] add salt
            salt: vec![],
        };

        let exec = Exec {
            args,
            url,
            verbosity,
            signer,
            transcoder,
        };

        async_std::task::block_on(async move { exec.exec(code, self.dry_run).await })
    }
}

struct InstantiateArgs {
    value: super::Balance,
    gas_limit: u64,
    storage_deposit_limit: Option<Balance>,
    data: Vec<u8>,
    salt: Vec<u8>,
}

pub struct Exec<'a> {
    args: InstantiateArgs,
    verbosity: Verbosity,
    url: url::Url,
    signer: subxt::PairSigner<DefaultConfig, sp_core::sr25519::Pair>,
    transcoder: ContractMessageTranscoder<'a>,
}

impl<'a> Exec<'a> {
    async fn subxt_api(&self) -> Result<api::RuntimeApi<DefaultConfig>> {
        let api = ClientBuilder::new()
            .set_url(self.url.to_string())
            .build()
            .await?
            .to_runtime_api::<api::RuntimeApi<DefaultConfig>>();
        Ok(api)
    }

    async fn exec(&self, code: Code, dry_run: bool) -> Result<()> {
        if dry_run {
            let result = self.instantiate_dry_run(code).await?;
            println!("{:?}", result); // todo: [AJ] extract relevant info?
            return Ok(());
        }

        match code {
            Code::Upload(code) => {
                let (code_hash, contract_account) = self.instantiate_with_code(code).await?;
                // todo: [AJ] prettify output
                println!("Code hash: {}", code_hash);
                println!("Contract account: {}", contract_account);
            }
            Code::Existing(code_hash) => {
                let contract_account = self.instantiate(code_hash).await?;
                // todo: [AJ] prettify output
                println!("Contract account: {}", contract_account);
            }
        }
        Ok(())
    }

    async fn instantiate_with_code(&self, code: Bytes) -> Result<(CodeHash, ContractAccount)> {
        let api = self.subxt_api().await?;
        let result = api
            .tx()
            .contracts()
            .instantiate_with_code(
                self.args.value,
                self.args.gas_limit,
                self.args.storage_deposit_limit,
                code.to_vec(),
                self.args.data.clone(),
                vec![], // todo! [AJ] add salt
            )
            .sign_and_submit_then_watch(&self.signer)
            .await?
            // todo: should we have optimistic fast mode just for InBlock?
            .wait_for_finalized_success()
            .await?;

        let metadata = api.client.metadata();

        display_events(&result, &self.transcoder, metadata, &self.verbosity)?;

        let code_stored = result
            .find_first_event::<api::contracts::events::CodeStored>()?
            .ok_or(anyhow::anyhow!("Failed to find CodeStored event"))?;
        let instantiated = result
            .find_first_event::<api::contracts::events::Instantiated>()?
            .ok_or(anyhow::anyhow!("Failed to find Instantiated event"))?;

        Ok((code_stored.code_hash, instantiated.contract))
    }

    async fn instantiate(&self, code_hash: CodeHash) -> Result<ContractAccount> {
        let api = self.subxt_api().await?;
        let result = api
            .tx()
            .contracts()
            .instantiate(
                self.args.value,
                self.args.gas_limit,
                self.args.storage_deposit_limit,
                code_hash,
                self.args.data.clone(),
                vec![], // todo! [AJ] add salt
            )
            .sign_and_submit_then_watch(&self.signer)
            .await?
            // todo: should we have optimistic fast mode just for InBlock?
            .wait_for_finalized_success()
            .await?;

        let metadata = api.client.metadata();
        display_events(&result, &self.transcoder, metadata, &self.verbosity)?;

        let instantiated = result
            .find_first_event::<api::contracts::events::Instantiated>()?
            .ok_or(anyhow::anyhow!("Failed to find Instantiated event"))?;

        Ok(instantiated.contract)
    }

    async fn instantiate_dry_run(&self, code: Code) -> Result<ContractInstantiateResult> {
        let url = self.url.to_string();
        let cli = WsClientBuilder::default().build(&url).await?;
        let call_request = InstantiateRequest {
            origin: self.signer.account_id().clone(),
            value: NumberOrHex::Hex(self.args.value.into()),
            gas_limit: NumberOrHex::Number(self.args.gas_limit),
            storage_deposit_limit: None, // todo: [AJ] call storage_deposit_limit
            code,
            data: self.args.data.clone().into(),
            salt: self.args.salt.clone().into(),
        };
        let params = vec![to_json_value(call_request)?];
        let result: ContractInstantiateResult = cli
            .request("contracts_instantiate", Some(params.into()))
            .await?;
        Ok(result)
    }
}

/// A struct that encodes RPC parameters required to instantiate a new smart-contract.
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

/// Reference to an existing code hash or a new wasm module.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum Code {
    /// A wasm module as raw bytes.
    Upload(Bytes),
    /// The code hash of an on-chain wasm blob.
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

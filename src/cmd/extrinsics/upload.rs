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

use super::{display_events, Balance, CodeHash, ContractMessageTranscoder, PairSigner, RuntimeApi};
use crate::ExtrinsicOpts;
use anyhow::{Context, Result};
use colored::Colorize;
use jsonrpsee::{
    types::{to_json_value, traits::Client as _},
    ws_client::WsClientBuilder,
};
use serde::Serialize;
use sp_core::Bytes;
use std::{fmt::Debug, path::PathBuf};
use structopt::StructOpt;
use subxt::{rpc::NumberOrHex, ClientBuilder, Config, DefaultConfig, Signer};

type CodeUploadResult = pallet_contracts_primitives::CodeUploadResult<CodeHash, Balance>;

#[derive(Debug, StructOpt)]
#[structopt(name = "upload", about = "Upload a contract's code")]
pub struct UploadCommand {
    /// Path to wasm contract code, defaults to `./target/ink/<name>.wasm`.
    #[structopt(parse(from_os_str))]
    wasm_path: Option<PathBuf>,
    #[structopt(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// The maximum amount of balance that can be charged from the caller to pay for the storage
    /// consumed.
    #[structopt(long)]
    storage_deposit_limit: Option<Balance>,
    /// Dry-run the code upload via rpc, instead of as an extrinsic. Code will not be uploaded.
    #[structopt(long, short = "rpc")]
    dry_run: bool,
}

impl UploadCommand {
    pub fn run(&self) -> Result<()> {
        let (crate_metadata, contract_metadata) =
            super::load_metadata(self.extrinsic_opts.manifest_path.as_ref())?;
        let transcoder = ContractMessageTranscoder::new(&contract_metadata);
        let signer = super::pair_signer(self.extrinsic_opts.signer()?);

        let wasm_path = match &self.wasm_path {
            Some(wasm_path) => wasm_path.clone(),
            None => crate_metadata.dest_wasm,
        };

        log::info!("Contract code path: {}", wasm_path.display());
        let code = std::fs::read(&wasm_path)
            .context(format!("Failed to read from {}", wasm_path.display()))?;

        async_std::task::block_on(async {
            if self.dry_run {
                self.upload_code_rpc(code, &signer).await
            } else {
                self.upload_code(code, &signer, &transcoder).await
            }
        })
    }

    async fn upload_code_rpc<'a>(&self, code: Vec<u8>, signer: &PairSigner) -> Result<()> {
        let url = self.extrinsic_opts.url.to_string();
        let cli = WsClientBuilder::default().build(&url).await?;
        let storage_deposit_limit = self
            .storage_deposit_limit
            .as_ref()
            .map(|limit| NumberOrHex::Hex((*limit).into()));
        let call_request = CodeUploadRequest {
            origin: signer.account_id().clone(),
            code: Bytes(code),
            storage_deposit_limit,
        };
        let params = vec![to_json_value(call_request)?];
        let result: CodeUploadResult = cli
            .request("contracts_upload_code", Some(params.into()))
            .await?;

        let exec_return_value =
            result.map_err(|e| anyhow::anyhow!("Failed to execute call via rpc: {:?}", e))?;
        println!(
            "{:>12} {:?}",
            "Code hash".bright_purple().bold(),
            exec_return_value.code_hash
        );
        println!(
            "{:>12} {:?}",
            "Deposit".bright_purple().bold(),
            exec_return_value.deposit
        );
        Ok(())
    }

    async fn upload_code<'a>(
        &self,
        code: Vec<u8>,
        signer: &PairSigner,
        transcoder: &ContractMessageTranscoder<'a>,
    ) -> Result<()> {
        let url = self.extrinsic_opts.url.to_string();
        let api = ClientBuilder::new()
            .set_url(&url)
            .build()
            .await?
            .to_runtime_api::<RuntimeApi>();

        let result = api
            .tx()
            .contracts()
            .upload_code(code, self.storage_deposit_limit)
            .sign_and_submit_then_watch(signer)
            .await?
            .wait_for_finalized_success()
            .await?;

        display_events(
            &result,
            &transcoder,
            api.client.metadata(),
            &self.extrinsic_opts.verbosity()?,
            self.extrinsic_opts.pretty_print,
        )
    }
}

/// A struct that encodes RPC parameters required for a call to upload a new code.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeUploadRequest {
    origin: <DefaultConfig as Config>::AccountId,
    code: Bytes,
    storage_deposit_limit: Option<NumberOrHex>,
}

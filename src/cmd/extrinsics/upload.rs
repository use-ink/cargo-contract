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
    display_events, runtime_api::api, wait_for_success_and_handle_error, Balance, CodeHash,
    ContractMessageTranscoder, ExtrinsicOpts, PairSigner, RuntimeApi,
};
use crate::name_value_println;
use anyhow::{Context, Result};
use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};
use serde::Serialize;
use sp_core::Bytes;
use std::{fmt::Debug, path::PathBuf};
use structopt::StructOpt;
use subxt::{rpc::NumberOrHex, ClientBuilder, Config, DefaultConfig, Signer};

type CodeUploadResult = pallet_contracts_primitives::CodeUploadResult<CodeHash, Balance>;
type CodeUploadReturnValue = pallet_contracts_primitives::CodeUploadReturnValue<CodeHash, Balance>;

#[derive(Debug, StructOpt)]
#[structopt(name = "upload", about = "Upload a contract's code")]
pub struct UploadCommand {
    /// Path to Wasm contract code, defaults to `./target/ink/<name>.wasm`.
    #[structopt(parse(from_os_str))]
    wasm_path: Option<PathBuf>,
    #[structopt(flatten)]
    extrinsic_opts: ExtrinsicOpts,
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
            if self.extrinsic_opts.dry_run {
                let result = self.upload_code_rpc(code, &signer).await?;

                name_value_println!("Code hash", format!("{:?}", result.code_hash));
                name_value_println!("Deposit", format!("{:?}", result.deposit));

                Ok(())
            } else {
                let code_stored = self.upload_code(code, &signer, &transcoder).await?;

                name_value_println!("Code hash", format!("{:?}", code_stored.code_hash));

                Ok(())
            }
        })
    }

    async fn upload_code_rpc(
        &self,
        code: Vec<u8>,
        signer: &PairSigner,
    ) -> Result<CodeUploadReturnValue> {
        let url = self.extrinsic_opts.url.to_string();
        let cli = WsClientBuilder::default().build(&url).await?;
        let storage_deposit_limit = self
            .extrinsic_opts
            .storage_deposit_limit
            .as_ref()
            .map(|limit| NumberOrHex::Hex((*limit).into()));
        let call_request = CodeUploadRequest {
            origin: signer.account_id().clone(),
            code: Bytes(code),
            storage_deposit_limit,
        };
        let params = rpc_params!(call_request);

        let result: CodeUploadResult = cli.request("contracts_upload_code", params).await?;

        result.map_err(|e| anyhow::anyhow!("Failed to execute call via rpc: {:?}", e))
    }

    async fn upload_code(
        &self,
        code: Vec<u8>,
        signer: &PairSigner,
        transcoder: &ContractMessageTranscoder<'_>,
    ) -> Result<api::contracts::events::CodeStored> {
        let url = self.extrinsic_opts.url.to_string();
        let api = ClientBuilder::new()
            .set_url(&url)
            .build()
            .await?
            .to_runtime_api::<RuntimeApi>();

        let tx_progress = api
            .tx()
            .contracts()
            .upload_code(code, self.extrinsic_opts.storage_deposit_limit)
            .sign_and_submit_then_watch(signer)
            .await?;

        let result = wait_for_success_and_handle_error(tx_progress).await?;

        display_events(
            &result,
            transcoder,
            api.client.metadata(),
            &self.extrinsic_opts.verbosity()?,
        )?;

        let code_stored = result
            .find_first::<api::contracts::events::CodeStored>()?
            .ok_or(anyhow::anyhow!("Failed to find CodeStored event"))?;

        Ok(code_stored)
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

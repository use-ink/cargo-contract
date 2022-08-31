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
    error_details,
    runtime_api::api,
    state_call,
    submit_extrinsic,
    Balance,
    Client,
    CodeHash,
    ContractMessageTranscoder,
    CrateMetadata,
    DefaultConfig,
    ExtrinsicOpts,
    PairSigner, events::parse_events,
};
use crate::name_value_println;
use anyhow::{
    Context,
    Result,
};
use colored::Colorize;

use scale::Encode;

use pallet_contracts_primitives::CodeUploadResult;
use std::{
    fmt::Debug,
    path::PathBuf,
};
use subxt::{
    Config,
    OnlineClient,
};

#[derive(Debug, clap::Args)]
#[clap(name = "upload", about = "Upload a contract's code")]
pub struct UploadCommand {
    /// Path to Wasm contract code, defaults to `./target/ink/<name>.wasm`.
    #[clap(parse(from_os_str))]
    wasm_path: Option<PathBuf>,
    #[clap(flatten)]
    extrinsic_opts: ExtrinsicOpts,
}

impl UploadCommand {
    pub fn run(&self) -> Result<()> {
        let crate_metadata = CrateMetadata::from_manifest_path(
            self.extrinsic_opts.manifest_path.as_ref(),
        )?;
        let transcoder = ContractMessageTranscoder::load(crate_metadata.metadata_path())?;
        let signer = super::pair_signer(self.extrinsic_opts.signer()?);

        let wasm_path = match &self.wasm_path {
            Some(wasm_path) => wasm_path.clone(),
            None => crate_metadata.dest_wasm,
        };

        tracing::debug!("Contract code path: {}", wasm_path.display());
        let code = std::fs::read(&wasm_path)
            .context(format!("Failed to read from {}", wasm_path.display()))?;

        async_std::task::block_on(async {
            let url = self.extrinsic_opts.url_to_string();
            let client = OnlineClient::from_url(url.clone()).await?;

            if self.extrinsic_opts.dry_run {
                match self.upload_code_rpc(code, &signer).await? {
                    Ok(result) => {
                        name_value_println!("Result", String::from("Success!"));
                        name_value_println!(
                            "Code hash",
                            format!("{:?}", result.code_hash)
                        );
                        name_value_println!("Deposit", format!("{:?}", result.deposit));
                    }
                    Err(err) => {
                        let metadata = client.metadata();
                        let err = error_details(&err, &metadata)?;
                        name_value_println!("Result", err);
                    }
                }
                Ok(())
            } else {
                if let Some(code_stored) = self
                    .upload_code(&client, code, &signer, &transcoder)
                    .await?
                {
                    name_value_println!(
                        "Code hash",
                        format!("{:?}", code_stored.code_hash)
                    );
                } else {
                    eprintln!(
                        "{} This contract has already been uploaded",
                        "warning:".yellow().bold(),
                    );
                }

                Ok(())
            }
        })
    }

    async fn upload_code_rpc(
        &self,
        code: Vec<u8>,
        signer: &PairSigner,
    ) -> Result<CodeUploadResult<CodeHash, Balance>> {
        let url = self.extrinsic_opts.url_to_string();
        let storage_deposit_limit = self.extrinsic_opts.storage_deposit_limit;
        let call_request = CodeUploadRequest {
            origin: signer.account_id().clone(),
            code,
            storage_deposit_limit,
        };
        state_call(&url, "ContractsApi_upload_code", call_request).await
    }

    async fn upload_code(
        &self,
        client: &Client,
        code: Vec<u8>,
        signer: &PairSigner,
        transcoder: &ContractMessageTranscoder,
    ) -> Result<Option<api::contracts::events::CodeStored>> {
        let call = super::runtime_api::api::tx()
            .contracts()
            .upload_code(code, self.extrinsic_opts.storage_deposit_limit);

        let result = submit_extrinsic(client, &call, signer).await?;

        let call_result = parse_events(
            &result,
            transcoder,
            &client.metadata(),
            Default::default()
        )?;

        let display = call_result.display(&self.extrinsic_opts.verbosity()?);
        println!("{}", display);

        let code_stored = result.find_first::<api::contracts::events::CodeStored>()?;

        Ok(code_stored)
    }
}

/// A struct that encodes RPC parameters required for a call to upload a new code.
#[derive(Encode)]
pub struct CodeUploadRequest {
    origin: <DefaultConfig as Config>::AccountId,
    code: Vec<u8>,
    storage_deposit_limit: Option<Balance>,
}

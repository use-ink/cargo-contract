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
    runtime_api::api,
    state_call,
    submit_extrinsic,
    Balance,
    Client,
    CodeHash,
    DefaultConfig,
    ExtrinsicOpts,
    PairSigner,
    TokenMetadata,
};
use crate::{
    cmd::extrinsics::{
        events::DisplayEvents,
        ErrorVariant,
    },
    name_value_println,
};
use anyhow::Result;
use contract_build::metadata::code_hash;
use pallet_contracts_primitives::CodeUploadResult;
use scale::Encode;
use std::{
    fmt::Debug,
    path::PathBuf,
};
use subxt::{
    Config,
    OnlineClient,
};

use super::runtime_api::api::runtime_types::pallet_contracts::wasm::Determinism;

#[derive(Debug, clap::Args)]
#[clap(name = "upload", about = "Upload a contract's code")]
pub struct UploadCommand {
    /// Path to Wasm contract code, defaults to `./target/ink/<name>.wasm`.
    #[clap(value_parser)]
    wasm_path: Option<PathBuf>,
    #[clap(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Export the call output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

impl UploadCommand {
    pub fn is_json(&self) -> bool {
        self.output_json
    }

    pub fn run(&self) -> Result<(), ErrorVariant> {
        let artifacts = self.extrinsic_opts.contract_artifacts()?;
        let signer = super::pair_signer(self.extrinsic_opts.signer()?);

        let code = artifacts
            .code
            .ok_or_else(|| anyhow::anyhow!("Contract code not found"))?; // todo: add more detail
        let code_hash = code_hash(&code);

        async_std::task::block_on(async {
            let url = self.extrinsic_opts.url_to_string();
            let client = OnlineClient::from_url(url.clone()).await?;

            if self.extrinsic_opts.dry_run {
                match self.upload_code_rpc(code, &client, &signer).await? {
                    Ok(result) => {
                        let upload_result = UploadDryRunResult {
                            result: String::from("Success!"),
                            code_hash: format!("{:?}", result.code_hash),
                            deposit: result.deposit,
                        };
                        if self.output_json {
                            println!("{}", upload_result.to_json()?);
                        } else {
                            upload_result.print();
                        }
                    }
                    Err(err) => {
                        let metadata = client.metadata();
                        let err = ErrorVariant::from_dispatch_error(&err, &metadata)?;
                        if self.output_json {
                            return Err(err)
                        } else {
                            name_value_println!("Result", err);
                        }
                    }
                }
                Ok(())
            } else if let Some(code_stored) =
                self.upload_code(&client, code, &signer).await?
            {
                let upload_result = UploadResult {
                    code_hash: format!("{:?}", code_stored.code_hash),
                };
                if self.output_json {
                    println!("{}", upload_result.to_json()?);
                } else {
                    upload_result.print();
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "This contract has already been uploaded with code hash: {code_hash:?}"
                )
                .into())
            }
        })
    }

    async fn upload_code_rpc(
        &self,
        code: Vec<u8>,
        client: &Client,
        signer: &PairSigner,
    ) -> Result<CodeUploadResult<CodeHash, Balance>> {
        let url = self.extrinsic_opts.url_to_string();
        let token_metadata = TokenMetadata::query(client).await?;
        let storage_deposit_limit = self
            .extrinsic_opts
            .storage_deposit_limit
            .as_ref()
            .map(|bv| bv.denominate_balance(&token_metadata))
            .transpose()?;
        let call_request = CodeUploadRequest {
            origin: signer.account_id().clone(),
            code,
            storage_deposit_limit,
            determinism: Determinism::Deterministic,
        };
        state_call(&url, "ContractsApi_upload_code", call_request).await
    }

    async fn upload_code(
        &self,
        client: &Client,
        code: Vec<u8>,
        signer: &PairSigner,
    ) -> Result<Option<api::contracts::events::CodeStored>, ErrorVariant> {
        let token_metadata = TokenMetadata::query(client).await?;
        let storage_deposit_limit =
            self.extrinsic_opts.storage_deposit_limit(&token_metadata)?;
        let call = super::runtime_api::api::tx().contracts().upload_code(
            code,
            storage_deposit_limit,
            Determinism::Deterministic,
        );

        let result = submit_extrinsic(client, &call, signer).await?;
        let display_events =
            DisplayEvents::from_events(&result, None, &client.metadata())?;

        let output = if self.output_json {
            display_events.to_json()?
        } else {
            let token_metadata = TokenMetadata::query(client).await?;
            display_events
                .display_events(self.extrinsic_opts.verbosity()?, &token_metadata)?
        };
        println!("{}", output);
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
    determinism: Determinism,
}

#[derive(serde::Serialize)]
pub struct UploadResult {
    code_hash: String,
}

#[derive(serde::Serialize)]
pub struct UploadDryRunResult {
    result: String,
    code_hash: String,
    deposit: Balance,
}

impl UploadResult {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn print(&self) {
        name_value_println!("Code hash", format!("{:?}", self.code_hash));
    }
}

impl UploadDryRunResult {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn print(&self) {
        name_value_println!("Result", self.result);
        name_value_println!("Code hash", format!("{:?}", self.code_hash));
        name_value_println!("Deposit", format!("{:?}", self.deposit));
    }
}

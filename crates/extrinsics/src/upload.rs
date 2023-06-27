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
    display_dry_run_result_warning,
    events::DisplayEvents,
    name_value_println,
    runtime_api::api::{
        self,
        runtime_types::pallet_contracts::wasm::Determinism,
    },
    state_call,
    submit_extrinsic,
    Balance,
    Client,
    CodeHash,
    DefaultConfig,
    ErrorVariant,
    ExtrinsicOpts,
    PairSigner,
    TokenMetadata,
    WasmCode,
};
use anyhow::Result;
use pallet_contracts_primitives::CodeUploadResult;
use scale::Encode;
use std::fmt::Debug;
use subxt::{
    Config,
    OnlineClient,
};

#[derive(Debug, clap::Args)]
#[clap(name = "upload", about = "Upload a contract's code")]
pub struct UploadCommand {
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

        let artifacts_path = artifacts.artifact_path().to_path_buf();
        let code = artifacts.code.ok_or_else(|| {
            anyhow::anyhow!(
                "Contract code not found from artifact file {}",
                artifacts_path.display()
            )
        })?;
        let code_hash = code.code_hash();

        async_std::task::block_on(async {
            let url = self.extrinsic_opts.url_to_string();
            let client = OnlineClient::from_url(url.clone()).await?;

            if !self.extrinsic_opts.execute {
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
                            display_dry_run_result_warning("upload");
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
            } else {
                let code_hash = hex::encode(code_hash);
                return Err(anyhow::anyhow!(
                    "This contract has already been uploaded with code hash: 0x{code_hash}"
                )
                .into());
            }
            Ok(())
        })
    }

    async fn upload_code_rpc(
        &self,
        code: WasmCode,
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
            code: code.0,
            storage_deposit_limit,
            determinism: Determinism::Enforced,
        };
        state_call(&url, "ContractsApi_upload_code", call_request).await
    }

    async fn upload_code(
        &self,
        client: &Client,
        code: WasmCode,
        signer: &PairSigner,
    ) -> Result<Option<api::contracts::events::CodeStored>, ErrorVariant> {
        let token_metadata = TokenMetadata::query(client).await?;
        let storage_deposit_limit =
            self.extrinsic_opts.storage_deposit_limit(&token_metadata)?;
        let call = crate::runtime_api::api::tx().contracts().upload_code(
            code.0,
            storage_deposit_limit,
            Determinism::Enforced,
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
        println!("{output}");
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

// Copyright (C) Use Ink (UK) Ltd.
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

use crate::{
    call_with_config,
    ErrorVariant,
};
use std::{
    fmt::{
        Debug,
        Display,
    },
    str::FromStr,
};

use super::{
    config::SignerConfig,
    display_dry_run_result_warning,
    parse_balance,
    prompt_confirm_unverifiable_upload,
    CLIExtrinsicOpts,
};
use anyhow::Result;
use contract_build::name_value_println;
use contract_extrinsics::{
    DisplayEvents,
    ExtrinsicOptsBuilder,
    TokenMetadata,
    UploadCommandBuilder,
    UploadExec,
};
use ink_env::Environment;
use serde::Serialize;
use subxt::{
    config::{
        DefaultExtrinsicParams,
        ExtrinsicParams,
        HashFor,
    },
    ext::{
        scale_decode::IntoVisitor,
        scale_encode::EncodeAsType,
    },
    Config,
};

#[derive(Debug, clap::Args)]
#[clap(name = "upload", about = "Upload a contract's code")]
pub struct UploadCommand {
    #[clap(flatten)]
    extrinsic_cli_opts: CLIExtrinsicOpts,
    /// Export the call output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

impl UploadCommand {
    /// Returns whether to export the call output in JSON format.
    pub fn output_json(&self) -> bool {
        self.output_json
    }

    pub async fn handle(&self) -> Result<(), ErrorVariant> {
        call_with_config!(
            self,
            run,
            self.extrinsic_cli_opts.chain_cli_opts.chain().config()
        )
    }

    async fn run<C: Config + Environment + SignerConfig<C>>(
        &self,
    ) -> Result<(), ErrorVariant>
    where
        <C as Config>::AccountId: IntoVisitor + FromStr + EncodeAsType,
        <<C as Config>::AccountId as FromStr>::Err: Display,
        C::Balance: Into<u128>
            + From<u128>
            + Display
            + Default
            + FromStr
            + Serialize
            + Debug
            + EncodeAsType,
        <C::ExtrinsicParams as ExtrinsicParams<C>>::Params:
            From<<DefaultExtrinsicParams<C> as ExtrinsicParams<C>>::Params>,
        HashFor<C>: IntoVisitor + EncodeAsType + From<[u8; 32]>,
    {
        let signer = C::Signer::from_str(&self.extrinsic_cli_opts.suri)
            .map_err(|_| anyhow::anyhow!("Failed to parse suri option"))?;
        let chain = self.extrinsic_cli_opts.chain_cli_opts.chain();
        let token_metadata = TokenMetadata::query::<C>(&chain.url()).await?;
        let storage_deposit_limit = self
            .extrinsic_cli_opts
            .storage_deposit_limit
            .clone()
            .map(|b| parse_balance(&b, &token_metadata))
            .transpose()
            .map_err(|e| {
                anyhow::anyhow!("Failed to parse storage_deposit_limit option: {}", e)
            })?;
        let extrinsic_opts = ExtrinsicOptsBuilder::new(signer)
            .file(self.extrinsic_cli_opts.file.clone())
            .manifest_path(self.extrinsic_cli_opts.manifest_path.clone())
            .url(chain.url())
            .storage_deposit_limit(storage_deposit_limit)
            .done();

        let mut upload_exec: UploadExec<C, C, _> =
            UploadCommandBuilder::new(extrinsic_opts).done().await?;
        let code_hash = upload_exec.code().code_hash(); // todo
        let metadata = upload_exec.client().metadata();

        if !self.extrinsic_cli_opts.execute {
            match upload_exec.upload_code_rpc().await? {
                Ok(result) => {
                    let upload_result = UploadDryRunResult {
                        result: String::from("Success!"),
                        code_hash: format!("{:?}", result.code_hash),
                        deposit: result.deposit,
                    };
                    if self.output_json() {
                        println!("{}", upload_result.to_json()?);
                    } else {
                        upload_result.print();
                        display_dry_run_result_warning("upload");
                    }
                }
                Err(err) => {
                    let err = ErrorVariant::from_dispatch_error(&err, &metadata)?;
                    if self.output_json() {
                        return Err(err)
                    } else {
                        name_value_println!("Result", err);
                    }
                }
            }
        } else {
            // A storage deposit needs to be provided for the extrinsic, if none is
            // given on the cli we execute a dry-run and use that.
            if storage_deposit_limit.is_none() {
                let limit = upload_exec.upload_code_rpc().await?
                    .unwrap_or_else(|err| {
                        panic!("No storage limit was given on the cli. We tried to fetch one via dry-run, but this failed: {err:?}");
                    });
                upload_exec.set_storage_deposit_limit(Some(limit.deposit));
            }

            if let Some(chain) = chain.production() {
                if !upload_exec.opts().contract_artifacts()?.is_verifiable() {
                    prompt_confirm_unverifiable_upload(&chain.to_string())?
                }
            }
            let upload_result = upload_exec.upload_code().await?;
            let display_events = DisplayEvents::from_events::<C, C>(
                &upload_result.events,
                None,
                &metadata,
            )?;
            let output_events = if self.output_json() {
                display_events.to_json()?
            } else {
                display_events.display_events::<C>(
                    self.extrinsic_cli_opts.verbosity()?,
                    &token_metadata,
                )?
            };
            let code_hash = hex::encode(code_hash);
            if self.output_json() {
                // Create a JSON object with the events and the code hash.
                let json_object = serde_json::json!({
                    "events": serde_json::from_str::<serde_json::Value>(&output_events)?,
                    "code_hash": code_hash,
                });
                println!("{}", serde_json::to_string_pretty(&json_object)?);
            } else {
                println!("{output_events}");
                name_value_println!("Code hash", format!("0x{code_hash}"));
            }
        }
        Ok(())
    }
}

#[derive(serde::Serialize)]
pub struct UploadDryRunResult<Balance> {
    pub result: String,
    pub code_hash: String,
    pub deposit: Balance,
}

impl<Balance> UploadDryRunResult<Balance>
where
    Balance: Debug + Serialize,
{
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn print(&self) {
        name_value_println!("Result", self.result);
        name_value_println!("Code hash", format!("{:?}", self.code_hash));
        name_value_println!("Deposit", format!("{:?}", self.deposit));
    }
}

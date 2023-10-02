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

use crate::ErrorVariant;
use std::fmt::Debug;

use super::{
    display_dry_run_result_warning,
    CLIExtrinsicOpts,
};
use anyhow::Result;
use contract_build::name_value_println;
use contract_extrinsics::{
    Balance,
    ExtrinsicOptsBuilder,
    UploadCommandBuilder,
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
        let extrinsic_opts = ExtrinsicOptsBuilder::default()
            .file(self.extrinsic_cli_opts.file.clone())
            .manifest_path(self.extrinsic_cli_opts.manifest_path.clone())
            .url(self.extrinsic_cli_opts.url.clone())
            .suri(self.extrinsic_cli_opts.suri.clone())
            .storage_deposit_limit(self.extrinsic_cli_opts.storage_deposit_limit.clone())
            .done();
        let upload_exec = UploadCommandBuilder::default()
            .extrinsic_opts(extrinsic_opts)
            .done()
            .await?;

        let code_hash = upload_exec.code().code_hash();

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
                    let metadata = upload_exec.client().metadata();
                    let err = ErrorVariant::from_dispatch_error(&err, &metadata)?;
                    if self.output_json() {
                        return Err(err)
                    } else {
                        name_value_println!("Result", err);
                    }
                }
            }
        } else {
            let upload_result = upload_exec.upload_code().await?;
            let display_events = upload_result.display_events;
            let output = if self.output_json() {
                display_events.to_json()?
            } else {
                display_events.display_events(
                    self.extrinsic_cli_opts.verbosity()?,
                    upload_exec.token_metadata(),
                )?
            };
            println!("{output}");
            if let Some(code_stored) = upload_result.code_stored {
                let upload_result = CodeHashResult {
                    code_hash: format!("{:?}", code_stored.code_hash),
                };
                if self.output_json() {
                    println!("{}", upload_result.to_json()?);
                } else {
                    upload_result.print();
                }
            } else {
                let code_hash = hex::encode(code_hash);
                return Err(anyhow::anyhow!(
                    "This contract has already been uploaded with code hash: 0x{code_hash}"
                )
                .into())
            }
        }
        Ok(())
    }
}

#[derive(serde::Serialize)]
pub struct CodeHashResult {
    pub code_hash: String,
}

impl CodeHashResult {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn print(&self) {
        name_value_println!("Code hash", format!("{:?}", self.code_hash));
    }
}

#[derive(serde::Serialize)]
pub struct UploadDryRunResult {
    pub result: String,
    pub code_hash: String,
    pub deposit: Balance,
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

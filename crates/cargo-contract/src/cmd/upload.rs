use crate::ErrorVariant;
use std::fmt::Debug;
use tokio::runtime::Runtime;

use anyhow::Result;
use contract_build::name_value_println;
use contract_extrinsics::{
    display_dry_run_result_warning,
    CodeHashResult,
    ExtrinsicOpts,
    TokenMetadata,
    UploadCommandBuilder,
    UploadDryRunResult,
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
    /// Returns whether to export the call output in JSON format.
    pub fn output_json(&self) -> bool {
        self.output_json
    }
}

pub fn handle_upload(upload_command: &UploadCommand) -> Result<(), ErrorVariant> {
    Runtime::new()?.block_on(async {
        let upload_exec = UploadCommandBuilder::default()
            .extrinsic_opts(upload_command.extrinsic_opts.clone())
            .output_json(upload_command.output_json())
            .done()
            .await;

        let code_hash = upload_exec.code().code_hash();

        if !upload_exec.opts().execute() {
            match upload_exec.upload_code_rpc().await? {
                Ok(result) => {
                    let upload_result = UploadDryRunResult {
                        result: String::from("Success!"),
                        code_hash: format!("{:?}", result.code_hash),
                        deposit: result.deposit,
                    };
                    if upload_exec.output_json() {
                        println!("{}", upload_result.to_json()?);
                    } else {
                        upload_result.print();
                        display_dry_run_result_warning("upload");
                    }
                }
                Err(err) => {
                    let metadata = upload_exec.client().metadata();
                    let err = ErrorVariant::from_dispatch_error(&err, &metadata)?;
                    if upload_exec.output_json() {
                        return Err(err)
                    } else {
                        name_value_println!("Result", err);
                    }
                }
            }
        } else {
            let upload_result = upload_exec.upload_code().await?;
            let display_events = upload_result.display_events;
            let output = if upload_exec.output_json() {
                display_events.to_json()?
            } else {
                let token_metadata = TokenMetadata::query(upload_exec.client()).await?;
                display_events
                    .display_events(upload_exec.opts().verbosity()?, &token_metadata)?
            };
            println!("{output}");
            if let Some(code_stored) = upload_result.code_stored {
                let upload_result = CodeHashResult {
                    code_hash: format!("{:?}", code_stored.code_hash),
                };
                if upload_exec.output_json() {
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
    })
}

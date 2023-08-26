use crate::ErrorVariant;
use std::fmt::Debug;

use super::CLIExtrinsicOpts;
use anyhow::Result;
use contract_build::name_value_println;
use contract_extrinsics::{
    parse_code_hash,
    DefaultConfig,
    ExtrinsicOptsBuilder,
    RemoveCommandBuilder,
    TokenMetadata,
};
use subxt::Config;

#[derive(Debug, clap::Args)]
#[clap(name = "remove", about = "Remove a contract's code")]
pub struct RemoveCommand {
    /// The hash of the smart contract code already uploaded to the chain.
    #[clap(long, value_parser = parse_code_hash)]
    code_hash: Option<<DefaultConfig as Config>::Hash>,
    #[clap(flatten)]
    extrinsic_cli_opts: CLIExtrinsicOpts,
    /// Export the call output as JSON.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

impl RemoveCommand {
    /// Returns whether to export the call output in JSON format.
    pub fn output_json(&self) -> bool {
        self.output_json
    }
}

pub async fn handle_remove(remove_command: &RemoveCommand) -> Result<(), ErrorVariant> {
    let extrinsic_opts = ExtrinsicOptsBuilder::default()
        .file(remove_command.extrinsic_cli_opts.file.clone())
        .manifest_path(remove_command.extrinsic_cli_opts.manifest_path.clone())
        .url(remove_command.extrinsic_cli_opts.url.clone())
        .suri(remove_command.extrinsic_cli_opts.suri.clone())
        .storage_deposit_limit(
            remove_command
                .extrinsic_cli_opts
                .storage_deposit_limit
                .clone(),
        )
        .skip_dry_run(remove_command.extrinsic_cli_opts.skip_dry_run)
        .done();
    let remove_exec = RemoveCommandBuilder::default()
        .code_hash(remove_command.code_hash)
        .extrinsic_opts(extrinsic_opts)
        .done()
        .await;
    let remove_result = remove_exec.remove_code().await?;
    let display_events = remove_result.display_events;
    let output = if remove_command.output_json() {
        display_events.to_json()?
    } else {
        let token_metadata = TokenMetadata::query(remove_exec.client()).await?;
        display_events.display_events(
            remove_command.extrinsic_cli_opts.verbosity().unwrap(),
            &token_metadata,
        )?
    };
    println!("{output}");
    if let Some(code_removed) = remove_result.code_removed {
        let remove_result = code_removed.code_hash;

        if remove_command.output_json() {
            println!("{}", &remove_result);
        } else {
            name_value_println!("Code hash", format!("{remove_result:?}"));
        }
        Result::<(), ErrorVariant>::Ok(())
    } else {
        let error_code_hash = hex::encode(remove_exec.final_code_hash());
        Err(anyhow::anyhow!(
            "Error removing the code for the supplied code hash: {}",
            error_code_hash
        )
        .into())
    }
}
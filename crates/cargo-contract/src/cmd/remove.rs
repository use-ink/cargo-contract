use crate::ErrorVariant;
use std::fmt::Debug;
use tokio::runtime::Runtime;

use anyhow::Result;
use contract_build::name_value_println;
use contract_extrinsics::{
    parse_code_hash,
    DefaultConfig,
    ExtrinsicOpts,
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
    extrinsic_opts: ExtrinsicOpts,
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

pub fn handle_remove(remove_command: &RemoveCommand) -> Result<(), ErrorVariant> {
    Runtime::new()?.block_on(async {
        let remove_exec = RemoveCommandBuilder::default()
            .code_hash(remove_command.code_hash)
            .extrinsic_opts(remove_command.extrinsic_opts.clone())
            .output_json(remove_command.output_json)
            .done()
            .await;
        let remove_result = remove_exec.remove_code().await?;
        let display_events = remove_result.display_events;
        let output = if remove_exec.output_json() {
            display_events.to_json()?
        } else {
            let token_metadata = TokenMetadata::query(remove_exec.client()).await?;
            display_events
                .display_events(remove_exec.opts().verbosity()?, &token_metadata)?
        };
        println!("{output}");
        if let Some(code_removed) = remove_result.code_removed {
            let remove_result = code_removed.code_hash;

            if remove_exec.output_json() {
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
    })
}

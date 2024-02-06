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
    parse_code_hash,
    CLIExtrinsicOpts,
};
use anyhow::Result;
use contract_build::name_value_println;
use contract_extrinsics::{
    DisplayEvents,
    ExtrinsicOptsBuilder,
    RemoveCommandBuilder,
    RemoveExec,
    TokenMetadata,
};
use ink_env::DefaultEnvironment;
use subxt::{
    Config,
    PolkadotConfig as DefaultConfig,
};

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

    pub async fn handle(&self) -> Result<(), ErrorVariant> {
        let token_metadata =
            TokenMetadata::query::<DefaultConfig>(&self.extrinsic_cli_opts.url).await?;

        let extrinsic_opts = ExtrinsicOptsBuilder::default()
            .file(self.extrinsic_cli_opts.file.clone())
            .manifest_path(self.extrinsic_cli_opts.manifest_path.clone())
            .url(self.extrinsic_cli_opts.url.clone())
            .suri(self.extrinsic_cli_opts.suri.clone())
            .storage_deposit_limit(
                self.extrinsic_cli_opts
                    .storage_deposit_limit
                    .clone()
                    .map(|bv| bv.denominate_balance(&token_metadata))
                    .transpose()?,
            )
            .done();
        let remove_exec: RemoveExec<DefaultConfig, DefaultEnvironment> =
            RemoveCommandBuilder::default()
                .code_hash(self.code_hash)
                .extrinsic_opts(extrinsic_opts)
                .done()
                .await?;
        let remove_result = remove_exec.remove_code().await?;
        let display_events =
            DisplayEvents::from_events::<DefaultConfig, DefaultEnvironment>(
                &remove_result.events,
                Some(remove_exec.transcoder()),
                &remove_exec.client().metadata(),
            )?;
        let output_events = if self.output_json() {
            display_events.to_json()?
        } else {
            display_events.display_events::<DefaultEnvironment>(
                self.extrinsic_cli_opts.verbosity().unwrap(),
                &token_metadata,
            )?
        };
        if let Some(code_removed) = remove_result.code_removed {
            let remove_result: <DefaultConfig as Config>::Hash = code_removed.code_hash;

            if self.output_json() {
                // Create a JSON object with the events and the removed code hash.
                let json_object = serde_json::json!({
                    "events": serde_json::from_str::<serde_json::Value>(&output_events)?,
                    "code_hash": remove_result,
                });
                let json_object = serde_json::to_string_pretty(&json_object)?;
                println!("{}", json_object);
            } else {
                println!("{}", output_events);
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
}

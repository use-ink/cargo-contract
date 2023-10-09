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

use super::CLIExtrinsicOpts;
use anyhow::Result;
use contract_build::name_value_println;
use contract_extrinsics::{
    parse_code_hash,
    DefaultConfig,
    ExtrinsicOptsBuilder,
    RemoveCommandBuilder,
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

    pub async fn handle(&self) -> Result<(), ErrorVariant> {
        let extrinsic_opts = ExtrinsicOptsBuilder::default()
            .file(self.extrinsic_cli_opts.file.clone())
            .manifest_path(self.extrinsic_cli_opts.manifest_path.clone())
            .url(self.extrinsic_cli_opts.url.clone())
            .suri(self.extrinsic_cli_opts.suri.clone())
            .storage_deposit_limit(self.extrinsic_cli_opts.storage_deposit_limit.clone())
            .done();
        let remove_exec = RemoveCommandBuilder::default()
            .code_hash(self.code_hash)
            .extrinsic_opts(extrinsic_opts)
            .done()
            .await?;
        let remove_result = remove_exec.remove_code().await?;
        let display_events = remove_result.display_events;
        let output = if self.output_json() {
            display_events.to_json()?
        } else {
            display_events.display_events(
                self.extrinsic_cli_opts.verbosity().unwrap(),
                remove_exec.token_metadata(),
            )?
        };
        println!("{output}");
        if let Some(code_removed) = remove_result.code_removed {
            let remove_result = code_removed.code_hash;

            if self.output_json() {
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
}

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
    parse_balance,
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
#[clap(name = "remove", about = "Remove a contract's code")]
pub struct RemoveCommand {
    /// The hash of the smart contract code already uploaded to the chain.
    #[clap(long)]
    code_hash: Option<String>,
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
            + IntoVisitor,
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
        let code_hash = self
            .code_hash
            .clone()
            .map(|h| parse_code_hash(&h))
            .transpose()
            .map_err(|e| anyhow::anyhow!("Failed to parse code_hash option: {}", e))?;
        let extrinsic_opts = ExtrinsicOptsBuilder::new(signer)
            .file(self.extrinsic_cli_opts.file.clone())
            .manifest_path(self.extrinsic_cli_opts.manifest_path.clone())
            .url(chain.url())
            .storage_deposit_limit(storage_deposit_limit)
            .done();

        let remove_exec: RemoveExec<C, C, _> = RemoveCommandBuilder::new(extrinsic_opts)
            .code_hash(code_hash)
            .done()
            .await?;
        let remove_events = remove_exec.remove_code().await?;
        let display_events = DisplayEvents::from_events::<C, C>(
            &remove_events,
            Some(remove_exec.transcoder()),
            &remove_exec.client().metadata(),
        )?;

        let output_events = if self.output_json() {
            display_events.to_json()?
        } else {
            display_events.display_events::<C>(
                self.extrinsic_cli_opts.verbosity().unwrap(),
                &token_metadata,
            )?
        };

        if self.output_json() {
            // Create a JSON object with the events and the removed code hash.
            let json_object = serde_json::json!({
                "events": serde_json::from_str::<serde_json::Value>(&output_events)?,
                "code_hash": code_hash,
            });
            let json_object = serde_json::to_string_pretty(&json_object)?;
            println!("{json_object}");
        } else {
            println!("{output_events}");
            name_value_println!("Code hash", format!("{code_hash:?}"));
        }
        Result::<(), ErrorVariant>::Ok(())
    }
}

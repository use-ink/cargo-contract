// Copyright 2018-2022 Parity Technologies (UK) Ltd.
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
    submit_extrinsic,
    Client,
    CodeHash,
    ContractMessageTranscoder,
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
use std::fmt::Debug;
use subxt::OnlineClient;

#[derive(Debug, clap::Args)]
#[clap(name = "remove", about = "Remove a contract's code")]
pub struct RemoveCommand {
    #[clap(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Export the call output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

impl RemoveCommand {
    pub fn is_json(&self) -> bool {
        self.output_json
    }

    pub fn run(&self) -> Result<(), ErrorVariant> {
        let artifacts = self.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;
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

            if let Some(code_removed) = self
                .remove_code(&client, sp_core::H256(code_hash), &signer, &transcoder)
                .await?
            {
                let remove_result = RemoveResult {
                    code_hash: format!("{:?}", code_removed.code_hash),
                };
                if self.output_json {
                    println!("{}", remove_result.to_json()?);
                } else {
                    remove_result.print();
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "This contract could not have been removed for the supplied code hash: {code_hash:?}")
                .into())
            }
        })
    }

    async fn remove_code(
        &self,
        client: &Client,
        code_hash: CodeHash,
        signer: &PairSigner,
        transcoder: &ContractMessageTranscoder,
    ) -> Result<Option<api::contracts::events::CodeRemoved>, ErrorVariant> {
        let call = super::runtime_api::api::tx()
            .contracts()
            .remove_code(sp_core::H256(code_hash.0));

        let result = submit_extrinsic(client, &call, signer).await?;
        let display_events =
            DisplayEvents::from_events(&result, Some(transcoder), &client.metadata())?;

        let output = if self.output_json {
            display_events.to_json()?
        } else {
            let token_metadata = TokenMetadata::query(client).await?;
            display_events
                .display_events(self.extrinsic_opts.verbosity()?, &token_metadata)?
        };
        println!("{}", output);
        let code_removed = result.find_first::<api::contracts::events::CodeRemoved>()?;
        Ok(code_removed)
    }
}

/// Reference to an existing code hash or a new Wasm module.
#[derive(serde::Serialize)]
pub struct RemoveResult {
    code_hash: String,
}
impl RemoveResult {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn print(&self) {
        name_value_println!("Code hash", format!("{:?}", self.code_hash));
    }
}
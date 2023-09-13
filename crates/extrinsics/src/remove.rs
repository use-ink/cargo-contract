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
    events::DisplayEvents,
    runtime_api::api::{
        self,
        contracts::events::CodeRemoved,
    },
    state,
    submit_extrinsic,
    url_to_string,
    Client,
    ContractMessageTranscoder,
    DefaultConfig,
    ErrorVariant,
    Missing,
};
use crate::extrinsic_opts::ExtrinsicOpts;
use anyhow::Result;
use core::marker::PhantomData;
use subxt::{
    Config,
    OnlineClient,
};
use subxt_signer::sr25519::Keypair;

pub struct RemoveOpts {
    code_hash: Option<<DefaultConfig as Config>::Hash>,
    extrinsic_opts: ExtrinsicOpts,
}

/// A builder for the remove command.
pub struct RemoveCommandBuilder<ExtrinsicOptions> {
    opts: RemoveOpts,
    marker: PhantomData<fn() -> ExtrinsicOptions>,
}

impl RemoveCommandBuilder<Missing<state::ExtrinsicOptions>> {
    /// Returns a clean builder for [`RemoveExec`].
    pub fn new() -> RemoveCommandBuilder<Missing<state::ExtrinsicOptions>> {
        RemoveCommandBuilder {
            opts: RemoveOpts {
                code_hash: None,
                extrinsic_opts: ExtrinsicOpts::default(),
            },
            marker: PhantomData,
        }
    }

    /// Sets the extrinsic operation.
    pub fn extrinsic_opts(
        self,
        extrinsic_opts: ExtrinsicOpts,
    ) -> RemoveCommandBuilder<state::ExtrinsicOptions> {
        RemoveCommandBuilder {
            opts: RemoveOpts {
                extrinsic_opts,
                ..self.opts
            },
            marker: PhantomData,
        }
    }
}

impl Default for RemoveCommandBuilder<Missing<state::ExtrinsicOptions>> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> RemoveCommandBuilder<E> {
    /// Sets the hash of the smart contract code already uploaded to the chain.
    pub fn code_hash(self, code_hash: Option<<DefaultConfig as Config>::Hash>) -> Self {
        let mut this = self;
        this.opts.code_hash = code_hash;
        this
    }
}

impl RemoveCommandBuilder<state::ExtrinsicOptions> {
    /// Preprocesses contract artifacts and options for subsequent removal of contract
    /// code.
    ///
    /// This function prepares the necessary data for removing contract code based on the
    /// provided contract artifacts and options. It ensures that the required code hash is
    /// available and sets up the client, signer, and other relevant parameters for the
    /// contract code removal operation.
    ///
    /// Returns the `RemoveExec` containing the preprocessed data for the contract code
    /// removal, or an error in case of failure.
    pub async fn done(self) -> Result<RemoveExec> {
        let artifacts = self.opts.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;
        let signer = self.opts.extrinsic_opts.signer()?;

        let artifacts_path = artifacts.artifact_path().to_path_buf();

        let final_code_hash = match (self.opts.code_hash.as_ref(), artifacts.code.as_ref()) {
            (Some(code_h), _) => Ok(code_h.0),
            (None, Some(_)) => artifacts.code_hash(),
            (None, None) => Err(anyhow::anyhow!(
                "No code_hash was provided or contract code was not found from artifact \
                file {}. Please provide a code hash with --code-hash argument or specify the \
                path for artifacts files with --manifest-path",
                artifacts_path.display()
            )),
        }?;
        let url = url_to_string(self.opts.extrinsic_opts.url());
        let client = OnlineClient::from_url(url.clone()).await?;

        Ok(RemoveExec {
            final_code_hash,
            opts: self.opts.extrinsic_opts.clone(),
            client,
            transcoder,
            signer,
        })
    }
}

pub struct RemoveExec {
    final_code_hash: [u8; 32],
    opts: ExtrinsicOpts,
    client: Client,
    transcoder: ContractMessageTranscoder,
    signer: Keypair,
}

impl RemoveExec {
    /// Removes a contract code from the blockchain.
    ///
    /// This function removes a contract code with the specified code hash from the
    /// blockchain, ensuring that it's no longer available for instantiation or
    /// execution. It interacts with the blockchain's runtime API to execute the
    /// removal operation and provides the resulting events from the removal.
    ///
    /// Returns the `RemoveResult` containing the events generated from the contract
    /// code removal, or an error in case of failure.
    pub async fn remove_code(&self) -> Result<RemoveResult, ErrorVariant> {
        let code_hash = sp_core::H256(self.final_code_hash);
        let call = api::tx()
            .contracts()
            .remove_code(sp_core::H256(code_hash.0));

        let result = submit_extrinsic(&self.client, &call, &self.signer).await?;
        let display_events = DisplayEvents::from_events(
            &result,
            Some(&self.transcoder),
            &self.client.metadata(),
        )?;

        let code_removed = result.find_first::<CodeRemoved>()?;
        Ok(RemoveResult {
            code_removed,
            display_events,
        })
    }

    /// Returns the final code hash.
    pub fn final_code_hash(&self) -> [u8; 32] {
        self.final_code_hash
    }

    /// Returns the extrinsic options.
    pub fn opts(&self) -> &ExtrinsicOpts {
        &self.opts
    }

    /// Returns the client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Returns the contract message transcoder.
    pub fn transcoder(&self) -> &ContractMessageTranscoder {
        &self.transcoder
    }

    /// Returns the signer.
    pub fn signer(&self) -> &Keypair {
        &self.signer
    }
}

pub struct RemoveResult {
    pub code_removed: Option<CodeRemoved>,
    pub display_events: DisplayEvents,
}

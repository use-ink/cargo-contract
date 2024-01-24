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
    events::{
        CodeRemoved,
        DisplayEvents,
    },
    state,
    submit_extrinsic,
    ContractMessageTranscoder,
    ErrorVariant,
    Missing,
    TokenMetadata,
};
use crate::{
    extrinsic_calls::RemoveCode,
    extrinsic_opts::ExtrinsicOpts,
};

use anyhow::Result;
use core::marker::PhantomData;
use ink_env::Environment;
use subxt::{
    backend::{
        legacy::LegacyRpcMethods,
        rpc::RpcClient,
    },
    config,
    ext::{
        scale_decode::IntoVisitor,
        scale_encode::EncodeAsType,
    },
    Config,
    OnlineClient,
};
use subxt_signer::sr25519::Keypair;

pub struct RemoveOpts<Hash> {
    code_hash: Option<Hash>,
    extrinsic_opts: ExtrinsicOpts,
}

/// A builder for the remove command.
pub struct RemoveCommandBuilder<C: Config, ExtrinsicOptions> {
    opts: RemoveOpts<C::Hash>,
    marker: PhantomData<fn() -> ExtrinsicOptions>,
}

impl<C: Config> RemoveCommandBuilder<C, Missing<state::ExtrinsicOptions>> {
    /// Returns a clean builder for [`RemoveExec`].
    pub fn new() -> RemoveCommandBuilder<C, Missing<state::ExtrinsicOptions>> {
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
    ) -> RemoveCommandBuilder<C, state::ExtrinsicOptions> {
        RemoveCommandBuilder {
            opts: RemoveOpts {
                extrinsic_opts,
                ..self.opts
            },
            marker: PhantomData,
        }
    }
}

impl<C: Config> Default for RemoveCommandBuilder<C, Missing<state::ExtrinsicOptions>> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Config, T> RemoveCommandBuilder<C, T> {
    /// Sets the hash of the smart contract code already uploaded to the chain.
    pub fn code_hash(self, code_hash: Option<C::Hash>) -> Self {
        let mut this = self;
        this.opts.code_hash = code_hash;
        this
    }
}

impl<C: Config> RemoveCommandBuilder<C, state::ExtrinsicOptions>
where
    C::Hash: From<[u8; 32]>,
{
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
    pub async fn done(self) -> Result<RemoveExec<C>> {
        let artifacts = self.opts.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;
        let signer = self.opts.extrinsic_opts.signer()?;

        let artifacts_path = artifacts.artifact_path().to_path_buf();

        let final_code_hash = match (self.opts.code_hash.as_ref(), artifacts.code.as_ref()) {
            (Some(code_h), _) => Ok(*code_h),
            (None, Some(_)) => artifacts.code_hash().map(|h| h.into() ),
            (None, None) => Err(anyhow::anyhow!(
                "No code_hash was provided or contract code was not found from artifact \
                file {}. Please provide a code hash with --code-hash argument or specify the \
                path for artifacts files with --manifest-path",
                artifacts_path.display()
            )),
        }?;

        let url = self.opts.extrinsic_opts.url();
        let rpc_cli = RpcClient::from_url(&url).await?;
        let client = OnlineClient::<C>::from_rpc_client(rpc_cli.clone()).await?;
        let rpc = LegacyRpcMethods::<C>::new(rpc_cli);

        let token_metadata = TokenMetadata::query(&rpc).await?;

        Ok(RemoveExec {
            final_code_hash,
            opts: self.opts.extrinsic_opts.clone(),
            rpc,
            client,
            transcoder,
            signer,
            token_metadata,
        })
    }
}

pub struct RemoveExec<C: Config> {
    final_code_hash: C::Hash,
    opts: ExtrinsicOpts,
    rpc: LegacyRpcMethods<C>,
    client: OnlineClient<C>,
    transcoder: ContractMessageTranscoder,
    signer: Keypair,
    token_metadata: TokenMetadata,
}

impl<C: Config> RemoveExec<C>
where
    C::Hash: IntoVisitor + EncodeAsType,
    C::AccountId: IntoVisitor + From<subxt_signer::sr25519::PublicKey>,
    C::Address: From<subxt_signer::sr25519::PublicKey>,
    C::Signature: From<subxt_signer::sr25519::Signature>,
    <C::ExtrinsicParams as config::ExtrinsicParams<C>>::OtherParams: Default,
{
    /// Removes a contract code from the blockchain.
    ///
    /// This function removes a contract code with the specified code hash from the
    /// blockchain, ensuring that it's no longer available for instantiation or
    /// execution. It interacts with the blockchain's runtime API to execute the
    /// removal operation and provides the resulting events from the removal.
    ///
    /// Returns the `RemoveResult` containing the events generated from the contract
    /// code removal, or an error in case of failure.
    pub async fn remove_code<E: Environment>(
        &self,
    ) -> Result<RemoveResult<C::Hash, C::AccountId, E::Balance>, ErrorVariant>
    where
        E::Balance: IntoVisitor,
    {
        let code_hash = self.final_code_hash;

        let call = RemoveCode::new(code_hash).build();

        let result =
            submit_extrinsic(&self.client, &self.rpc, &call, &self.signer).await?;
        let display_events = DisplayEvents::from_events(
            &result,
            Some(&self.transcoder),
            &self.client.metadata(),
        )?;

        let code_removed =
            result.find_first::<CodeRemoved<C::Hash, C::AccountId, E::Balance>>()?;
        Ok(RemoveResult {
            code_removed,
            display_events,
        })
    }

    /// Returns the final code hash.
    pub fn final_code_hash(&self) -> C::Hash {
        self.final_code_hash
    }

    /// Returns the extrinsic options.
    pub fn opts(&self) -> &ExtrinsicOpts {
        &self.opts
    }

    /// Returns the client.
    pub fn client(&self) -> &OnlineClient<C> {
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

    /// Returns the token metadata.
    pub fn token_metadata(&self) -> &TokenMetadata {
        &self.token_metadata
    }
}

pub struct RemoveResult<Hash, AccountId, Balance> {
    pub code_removed: Option<CodeRemoved<Hash, AccountId, Balance>>,
    pub display_events: DisplayEvents,
}

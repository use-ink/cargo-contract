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

use super::{
    submit_extrinsic,
    ContractMessageTranscoder,
    ErrorVariant,
};
use crate::{
    extrinsic_calls::RemoveCode,
    extrinsic_opts::ExtrinsicOpts,
};

use anyhow::Result;
use ink_env::Environment;
use subxt::{
    backend::{
        legacy::LegacyRpcMethods,
        rpc::RpcClient,
    },
    blocks::ExtrinsicEvents,
    config::{
        DefaultExtrinsicParams,
        ExtrinsicParams,
    },
    ext::{
        scale_decode::IntoVisitor,
        scale_encode::EncodeAsType,
    },
    tx,
    utils::H256,
    Config,
    OnlineClient,
};

/// A builder for the remove command.
pub struct RemoveCommandBuilder<C: Config, E: Environment, Signer: Clone> {
    code_hash: Option<H256>,
    extrinsic_opts: ExtrinsicOpts<C, E, Signer>,
}

impl<C: Config, E: Environment, Signer> RemoveCommandBuilder<C, E, Signer>
where
    Signer: tx::Signer<C> + Clone,
{
    /// Returns a clean builder for [`RemoveExec`].
    pub fn new(
        extrinsic_opts: ExtrinsicOpts<C, E, Signer>,
    ) -> RemoveCommandBuilder<C, E, Signer> {
        RemoveCommandBuilder {
            code_hash: None,
            extrinsic_opts,
        }
    }

    /// Sets the hash of the smart contract code already uploaded to the chain.
    pub fn code_hash(self, code_hash: Option<H256>) -> Self {
        let mut this = self;
        this.code_hash = code_hash;
        this
    }
}

impl<C: Config, E: Environment, Signer> RemoveCommandBuilder<C, E, Signer>
where
    C::Hash: From<[u8; 32]>,
    Signer: tx::Signer<C> + Clone,
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
    pub async fn done(self) -> Result<RemoveExec<C, E, Signer>> {
        let artifacts = self.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;

        let artifacts_path = artifacts.artifact_path().to_path_buf();

        let final_code_hash = match (self.code_hash.as_ref(), artifacts.contract_binary.as_ref()) {
            (Some(code_h), _) => Ok(*code_h),
            (None, Some(_)) => artifacts.code_hash().map(|h| h.into() ),
            (None, None) => Err(anyhow::anyhow!(
                "No code_hash was provided or contract code was not found from artifact \
                file {}. Please provide a code hash with --code-hash argument or specify the \
                path for artifacts files with --manifest-path",
                artifacts_path.display()
            )),
        }?;

        let url = self.extrinsic_opts.url();
        let rpc_cli = RpcClient::from_url(&url).await?;
        let client = OnlineClient::<C>::from_rpc_client(rpc_cli.clone()).await?;
        let rpc = LegacyRpcMethods::<C>::new(rpc_cli);

        Ok(RemoveExec {
            final_code_hash,
            opts: self.extrinsic_opts,
            rpc,
            client,
            transcoder,
        })
    }
}

pub struct RemoveExec<C: Config, E: Environment, Signer: Clone> {
    final_code_hash: H256,
    opts: ExtrinsicOpts<C, E, Signer>,
    rpc: LegacyRpcMethods<C>,
    client: OnlineClient<C>,
    transcoder: ContractMessageTranscoder,
}

impl<C: Config, E: Environment, Signer> RemoveExec<C, E, Signer>
where
    C::Hash: IntoVisitor + EncodeAsType,
    C::AccountId: IntoVisitor,
    <C::ExtrinsicParams as ExtrinsicParams<C>>::Params:
        From<<DefaultExtrinsicParams<C> as ExtrinsicParams<C>>::Params>,
    Signer: tx::Signer<C> + Clone,
{
    /// Removes a contract code from the blockchain.
    ///
    /// This function removes a contract code with the specified code hash from the
    /// blockchain, ensuring that it's no longer available for instantiation or
    /// execution. It interacts with the blockchain's runtime API to execute the
    /// removal operation and provides the resulting events from the removal.
    ///
    /// Returns the events generated from the contract code removal, or an error
    /// in case of failure.
    pub async fn remove_code(&self) -> Result<ExtrinsicEvents<C>, ErrorVariant>
    where
        E::Balance: IntoVisitor + Into<u128>,
    {
        let code_hash = self.final_code_hash;
        let call = RemoveCode::new(code_hash).build();
        let events =
            submit_extrinsic(&self.client, &self.rpc, &call, self.opts.signer()).await?;
        Ok(events)
    }

    /// Returns the final code hash.
    pub fn final_code_hash(&self) -> H256 {
        self.final_code_hash
    }

    /// Returns the extrinsic options.
    pub fn opts(&self) -> &ExtrinsicOpts<C, E, Signer> {
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
}

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
    pallet_revive_primitives::CodeUploadResult,
    state_call,
    submit_extrinsic,
    ContractBinary,
    ErrorVariant,
};
use crate::{
    check_env_types,
    extrinsic_calls::UploadCode,
    extrinsic_opts::ExtrinsicOpts,
};
use anyhow::Result;
use contract_transcode::ContractMessageTranscoder;
use ink_env::Environment;
use scale::Encode;
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
    Config,
    OnlineClient,
};

/// A builder for the upload command.
pub struct UploadCommandBuilder<C: Config, E: Environment, Signer: Clone> {
    extrinsic_opts: ExtrinsicOpts<C, E, Signer>,
}

impl<C: Config, E: Environment, Signer> UploadCommandBuilder<C, E, Signer>
where
    Signer: tx::Signer<C> + Clone,
{
    /// Returns a clean builder for [`UploadExec`].
    pub fn new(
        extrinsic_opts: ExtrinsicOpts<C, E, Signer>,
    ) -> UploadCommandBuilder<C, E, Signer> {
        UploadCommandBuilder { extrinsic_opts }
    }

    /// Preprocesses contract artifacts and options for subsequent upload.
    ///
    /// This function prepares the necessary data for uploading a contract
    /// based on the provided contract artifacts and options. It ensures that the
    /// required contract code is available and sets up the client and signer for the
    /// operation.
    ///
    /// Returns the `UploadExec` containing the preprocessed data for the upload or
    /// execution.
    pub async fn done(self) -> Result<UploadExec<C, E, Signer>> {
        let artifacts = self.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;

        let artifacts_path = artifacts.artifact_path().to_path_buf();
        let code = artifacts.contract_binary.ok_or_else(|| {
            anyhow::anyhow!(
                "Contract code not found from artifact file {}",
                artifacts_path.display()
            )
        })?;

        let url = self.extrinsic_opts.url();
        let rpc_cli = RpcClient::from_url(&url).await?;
        let client = OnlineClient::from_rpc_client(rpc_cli.clone()).await?;
        check_env_types(&client, &transcoder, self.extrinsic_opts.verbosity())?;
        let rpc = LegacyRpcMethods::new(rpc_cli);

        Ok(UploadExec {
            opts: self.extrinsic_opts,
            rpc,
            client,
            code,
            transcoder,
        })
    }
}

pub struct UploadExec<C: Config, E: Environment, Signer: Clone> {
    opts: ExtrinsicOpts<C, E, Signer>,
    rpc: LegacyRpcMethods<C>,
    client: OnlineClient<C>,
    code: ContractBinary,
    transcoder: ContractMessageTranscoder,
}

impl<C: Config, E: Environment, Signer> UploadExec<C, E, Signer>
where
    C::Hash: IntoVisitor,
    C::AccountId: IntoVisitor,
    E::Balance: IntoVisitor + EncodeAsType,
    <C::ExtrinsicParams as ExtrinsicParams<C>>::Params:
        From<<DefaultExtrinsicParams<C> as ExtrinsicParams<C>>::Params>,
    Signer: tx::Signer<C> + Clone,
{
    /// Uploads contract code to a specified URL using a JSON-RPC call.
    ///
    /// This function performs a JSON-RPC call to upload contract code to the given URL.
    /// It constructs a [`CodeUploadRequest`] with the code and relevant parameters,
    /// then sends the request using the provided URL. This operation does not modify
    /// the state of the blockchain.
    pub async fn upload_code_rpc(&self) -> Result<CodeUploadResult<E::Balance>> {
        let storage_deposit_limit = self.opts.storage_deposit_limit();
        let call_request = CodeUploadRequest {
            origin: self.opts.signer().account_id(),
            code: self.code.0.clone(),
            storage_deposit_limit,
        };
        state_call(&self.rpc, "ReviveApi_upload_code", call_request).await
    }

    /// Uploads contract code to the blockchain with specified options.
    ///
    /// This function facilitates the process of uploading contract code to the
    /// blockchain, utilizing the provided options.
    /// The function handles the necessary interactions with the blockchain's runtime
    /// API to ensure the successful upload of the code.
    pub async fn upload_code(&self) -> Result<UploadResult<C>, ErrorVariant> {
        let storage_deposit_limit = self.opts.storage_deposit_limit();

        let call = UploadCode::new(
            self.code.clone(),
            storage_deposit_limit.expect("no storage deposit limit available"),
        )
        .build();

        let events =
            submit_extrinsic(&self.client, &self.rpc, &call, self.opts.signer()).await?;
        tracing::debug!("events: {:?}", events);

        // The extrinsic will succeed for those two cases:
        //   - the code was already uploaded before.
        //   - the code was uploaded now.

        Ok(UploadResult { events })
    }

    /// Returns the extrinsic options.
    pub fn opts(&self) -> &ExtrinsicOpts<C, E, Signer> {
        &self.opts
    }

    /// Returns the client.
    pub fn client(&self) -> &OnlineClient<C> {
        &self.client
    }

    /// Returns the code.
    pub fn code(&self) -> &ContractBinary {
        &self.code
    }

    /// Returns the contract message transcoder.
    pub fn transcoder(&self) -> &ContractMessageTranscoder {
        &self.transcoder
    }

    /// Sets a new storage deposit limit.
    pub fn set_storage_deposit_limit(&mut self, limit: Option<E::Balance>) {
        self.opts.set_storage_deposit_limit(limit);
    }
}

/// A struct that encodes RPC parameters required for a call to upload a new code.
#[derive(Encode)]
struct CodeUploadRequest<AccountId, Balance> {
    origin: AccountId,
    code: Vec<u8>,
    storage_deposit_limit: Option<Balance>,
}

/// A struct representing the result of an upload command execution.
pub struct UploadResult<C: Config> {
    pub events: ExtrinsicEvents<C>,
}

/// Copied from `pallet-contracts` to additionally implement `scale_encode::EncodeAsType`.
#[allow(dead_code)]
#[derive(Debug, Encode, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub(crate) enum Determinism {
    /// The execution should be deterministic and hence no indeterministic instructions
    /// are allowed.
    ///
    /// Dispatchables always use this mode in order to make on-chain execution
    /// deterministic.
    Enforced,
    /// Allow calling or uploading an indeterministic code.
    ///
    /// This is only possible when calling into `pallet-contracts` directly via
    /// [`crate::Pallet::bare_call`].
    ///
    /// # Note
    ///
    /// **Never** use this mode for on-chain execution.
    Relaxed,
}

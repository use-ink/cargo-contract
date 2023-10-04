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
    account_id,
    events::DisplayEvents,
    runtime_api::api::{
        contracts::events::CodeStored,
        runtime_types::pallet_contracts::wasm::Determinism,
    },
    state,
    state_call,
    submit_extrinsic,
    Balance,
    Client,
    CodeHash,
    DefaultConfig,
    ErrorVariant,
    Missing,
    TokenMetadata,
    WasmCode,
};
use crate::extrinsic_opts::ExtrinsicOpts;
use anyhow::Result;
use core::marker::PhantomData;
use pallet_contracts_primitives::CodeUploadResult;
use scale::Encode;
use subxt::{
    backend::{
        legacy::LegacyRpcMethods,
        rpc::RpcClient,
    },
    Config,
    OnlineClient,
};
use subxt_signer::sr25519::Keypair;

struct UploadOpts {
    extrinsic_opts: ExtrinsicOpts,
}

/// A builder for the upload command.
pub struct UploadCommandBuilder<ExtrinsicOptions> {
    opts: UploadOpts,
    marker: PhantomData<fn() -> ExtrinsicOptions>,
}

impl UploadCommandBuilder<Missing<state::ExtrinsicOptions>> {
    /// Returns a clean builder for [`UploadExec`].
    pub fn new() -> UploadCommandBuilder<Missing<state::ExtrinsicOptions>> {
        UploadCommandBuilder {
            opts: UploadOpts {
                extrinsic_opts: ExtrinsicOpts::default(),
            },
            marker: PhantomData,
        }
    }

    /// Sets the extrinsic operation.
    pub fn extrinsic_opts(
        self,
        extrinsic_opts: ExtrinsicOpts,
    ) -> UploadCommandBuilder<state::ExtrinsicOptions> {
        UploadCommandBuilder {
            opts: UploadOpts { extrinsic_opts },
            marker: PhantomData,
        }
    }
}

impl Default for UploadCommandBuilder<Missing<state::ExtrinsicOptions>> {
    fn default() -> Self {
        Self::new()
    }
}

impl UploadCommandBuilder<state::ExtrinsicOptions> {
    /// Preprocesses contract artifacts and options for subsequent upload.
    ///
    /// This function prepares the necessary data for uploading a contract
    /// based on the provided contract artifacts and options. It ensures that the
    /// required contract code is available and sets up the client and signer for the
    /// operation.
    ///
    /// Returns the `UploadExec` containing the preprocessed data for the upload or
    /// execution.
    pub async fn done(self) -> Result<UploadExec> {
        let artifacts = self.opts.extrinsic_opts.contract_artifacts()?;
        let signer = self.opts.extrinsic_opts.signer()?;

        let artifacts_path = artifacts.artifact_path().to_path_buf();
        let code = artifacts.code.ok_or_else(|| {
            anyhow::anyhow!(
                "Contract code not found from artifact file {}",
                artifacts_path.display()
            )
        })?;

        let url = self.opts.extrinsic_opts.url();
        let rpc_cli = RpcClient::from_url(&url).await?;
        let client = OnlineClient::from_rpc_client(rpc_cli.clone()).await?;
        let rpc = LegacyRpcMethods::new(rpc_cli);

        let token_metadata = TokenMetadata::query(&rpc).await?;

        Ok(UploadExec {
            opts: self.opts.extrinsic_opts.clone(),
            rpc,
            client,
            code,
            signer,
            token_metadata,
        })
    }
}

pub struct UploadExec {
    opts: ExtrinsicOpts,
    rpc: LegacyRpcMethods<DefaultConfig>,
    client: Client,
    code: WasmCode,
    signer: Keypair,
    token_metadata: TokenMetadata,
}

impl UploadExec {
    /// Uploads contract code to a specified URL using a JSON-RPC call.
    ///
    /// This function performs a JSON-RPC call to upload contract code to the given URL.
    /// It constructs a [`CodeUploadRequest`] with the code and relevant parameters,
    /// then sends the request using the provided URL. This operation does not modify
    /// the state of the blockchain.
    pub async fn upload_code_rpc(&self) -> Result<CodeUploadResult<CodeHash, Balance>> {
        let storage_deposit_limit = self
            .opts
            .storage_deposit_limit()
            .as_ref()
            .map(|bv| bv.denominate_balance(&self.token_metadata))
            .transpose()?;
        let call_request = CodeUploadRequest {
            origin: account_id(&self.signer),
            code: self.code.0.clone(),
            storage_deposit_limit,
            determinism: Determinism::Enforced,
        };
        state_call(self.rpc.clone(), "ContractsApi_upload_code", call_request).await
    }

    /// Uploads contract code to the blockchain with specified options.
    ///
    /// This function facilitates the process of uploading contract code to the
    /// blockchain, utilizing the provided options.
    /// The function handles the necessary interactions with the blockchain's runtime
    /// API to ensure the successful upload of the code.
    pub async fn upload_code(&self) -> Result<UploadResult, ErrorVariant> {
        let storage_deposit_limit = self
            .opts
            .compact_storage_deposit_limit(&self.token_metadata)?;
        let call = crate::runtime_api::api::tx().contracts().upload_code(
            self.code.0.clone(),
            storage_deposit_limit,
            Determinism::Enforced,
        );

        let result =
            submit_extrinsic(&self.client, &self.rpc, &call, &self.signer).await?;
        let display_events =
            DisplayEvents::from_events(&result, None, &self.client.metadata())?;

        let code_stored = result.find_first::<CodeStored>()?;
        Ok(UploadResult {
            code_stored,
            display_events,
        })
    }

    /// Returns the extrinsic options.
    pub fn opts(&self) -> &ExtrinsicOpts {
        &self.opts
    }

    /// Returns the client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Returns the code.
    pub fn code(&self) -> &WasmCode {
        &self.code
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

/// A struct that encodes RPC parameters required for a call to upload a new code.
#[derive(Encode)]
pub struct CodeUploadRequest {
    origin: <DefaultConfig as Config>::AccountId,
    code: Vec<u8>,
    storage_deposit_limit: Option<Balance>,
    determinism: Determinism,
}

pub struct UploadResult {
    pub code_stored: Option<CodeStored>,
    pub display_events: DisplayEvents,
}

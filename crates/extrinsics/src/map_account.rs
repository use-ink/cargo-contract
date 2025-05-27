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
    dry_run_extrinsic,
    pallet_revive_primitives::StorageDeposit,
    submit_extrinsic,
    AccountIdMapper,
    ContractMessageTranscoder,
    ErrorVariant,
};
use crate::{
    check_env_types,
    extrinsic_opts::ExtrinsicOpts,
};
use anyhow::Result;
use contract_transcode::Value;
use ink_env::Environment;
use serde::Serialize;

use crate::extrinsic_calls::MapAccount;
use scale::Encode;
use sp_weights::Weight;
use std::marker::PhantomData;
use subxt::{
    backend::{
        legacy::{
            rpc_methods::DryRunResult,
            LegacyRpcMethods,
        },
        rpc::RpcClient,
    },
    blocks::ExtrinsicEvents,
    config::{
        DefaultExtrinsicParams,
        ExtrinsicParams,
    },
    ext::subxt_rpcs::methods::legacy::DryRunDecodeError,
    tx,
    utils::H160,
    Config,
    OnlineClient,
};

/// A builder for the instantiate command.
pub struct MapAccountCommandBuilder<C: Config, E: Environment, Signer: Clone> {
    extrinsic_opts: ExtrinsicOpts<C, E, Signer>,
    #[allow(clippy::type_complexity)]
    _phantom: PhantomData<fn() -> (C, E, Signer)>,
}

impl<C: Config, E: Environment, Signer> MapAccountCommandBuilder<C, E, Signer>
where
    //E::Balance: Default,
    Signer: tx::Signer<C> + Clone,
    //C::Hash: From<[u8; 32]>,
{
    /// Returns a clean builder for [`InstantiateExec`].
    pub fn new(
        extrinsic_opts: ExtrinsicOpts<C, E, Signer>,
    ) -> MapAccountCommandBuilder<C, E, Signer> {
        MapAccountCommandBuilder {
            extrinsic_opts,
            _phantom: Default::default(),
        }
    }

    /// Preprocesses contract artifacts and options for instantiation.
    ///
    /// This function prepares the required data for instantiating a contract based on the
    /// provided contract artifacts and options. It ensures that the necessary contract
    /// code is available, sets up the client, signer, and other relevant parameters,
    /// preparing for the instantiation process.
    ///
    /// Returns the [`InstantiateExec`] containing the preprocessed data for the
    /// instantiation, or an error in case of failure.
    pub async fn done(self) -> Result<MapAccountExec<C, E, Signer>> {
        let url = self.extrinsic_opts.url();
        let artifacts = self.extrinsic_opts.contract_artifacts()?;
        let transcoder = artifacts.contract_transcoder()?;
        /*
        let data = transcoder.encode(&self.constructor, &self.args)?;
        let code = if let Some(code) = artifacts.code {
            Code::Upload(code.0)
        } else {
            let code_hash = artifacts.code_hash()?;
            Code::Existing(code_hash.into())
        };
        let salt = self.salt.clone().map(|s| {
            let bytes = s.0 ;
            assert!(bytes.len() <= 32, "salt has to be <= 32 bytes");
            let mut salt = [0u8; 32];
            salt[..bytes.len()].copy_from_slice(&bytes[..bytes.len()]);
            salt
        });
         */

        let rpc_cli = RpcClient::from_url(&url).await?;
        let client = OnlineClient::from_rpc_client(rpc_cli.clone()).await?;
        check_env_types(&client, &transcoder, self.extrinsic_opts.verbosity())?;
        let rpc = LegacyRpcMethods::new(rpc_cli);

        Ok(MapAccountExec {
            opts: self.extrinsic_opts,
            rpc,
            client,
            transcoder,
        })
    }
}

pub struct MapAccountExec<C: Config, E: Environment, Signer: Clone> {
    opts: ExtrinsicOpts<C, E, Signer>,
    rpc: LegacyRpcMethods<C>,
    client: OnlineClient<C>,
    transcoder: ContractMessageTranscoder,
}

impl<C: Config, E: Environment, Signer> MapAccountExec<C, E, Signer>
where
    <C::ExtrinsicParams as ExtrinsicParams<C>>::Params:
        From<<DefaultExtrinsicParams<C> as ExtrinsicParams<C>>::Params>,
    Signer: tx::Signer<C> + Clone,
{
    /// Simulates a contract instantiation without modifying the blockchain.
    ///
    /// This function performs a dry run simulation of a contract instantiation, capturing
    /// essential information such as the contract address, gas consumption, and storage
    /// deposit. The simulation is executed without actually executing the
    /// instantiation on the blockchain.
    ///
    /// Returns the dry run simulation result, or an error in case of failure.
    pub async fn map_account_dry_run(&self) -> Result<u128> {
        let call = MapAccount::new().build();
        let (bytes, partial_fee_estimation) =
            dry_run_extrinsic(&self.client, &self.rpc, &call, self.opts.signer()).await?;
        let res = bytes.into_dry_run_result();
        match res {
            Ok(DryRunResult::Success) => Ok(partial_fee_estimation),
            Ok(DryRunResult::DispatchError(err)) => {
                Err(anyhow::format_err!("dispatch error: {:?}", err))
            }
            Ok(DryRunResult::TransactionValidityError) => {
                // todo seems like an external bug: https://github.com/paritytech/polkadot-sdk/issues/7305
                // Err(anyhow::format_err!("validity err"))
                Ok(partial_fee_estimation)
            }
            Err(err) => {
                match err {
                    DryRunDecodeError::WrongNumberOfBytes => {
                        Err(anyhow::anyhow!("decode error: dry run result was less than 2 bytes, which is invalid"))
                    }
                    DryRunDecodeError::InvalidBytes => Err(anyhow::anyhow!("decode error: dry run bytes are not valid"))
                }
            }
        }
    }

    /// Initiates the deployment of a smart contract on the blockchain.
    ///
    /// This function can be used to deploy a contract using either its source code or an
    /// existing code hash. It triggers the instantiation process by submitting an
    /// extrinsic with the specified gas limit, storage deposit, code or code hash,
    /// input data, and salt.
    ///
    /// The deployment result provides essential information about the instantiation,
    /// encapsulated in an [`InstantiateExecResult`] object, including the contract's
    /// result, contract address, and token metadata.
    pub async fn map_account(&self) -> Result<MapAccountExecResult<C>, ErrorVariant> {
        let call = MapAccount::new().build();
        let events =
            submit_extrinsic(&self.client, &self.rpc, &call, self.opts.signer()).await?;
        let account_id = self.opts.signer().account_id();
        Ok(MapAccountExecResult {
            events,
            address: AccountIdMapper::to_address(&account_id.encode()[..]),
        })
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

/// A struct representing the result of an instantiate command execution.
pub struct MapAccountExecResult<C: Config> {
    pub events: ExtrinsicEvents<C>,
    //pub code_hash: Option<H256>,
    pub address: H160,
}

/// Result of the contract call
#[derive(serde::Serialize)]
pub struct MapAccountDryRunResult<Balance: Serialize> {
    /// The decoded result returned from the constructor
    pub result: Value,
    /// contract address
    pub contract: String,
    /// Was the operation reverted
    pub reverted: bool,
    pub gas_consumed: Weight,
    pub gas_required: Weight,
    /// Storage deposit after the operation
    pub storage_deposit: StorageDeposit<Balance>,
}

impl<Balance: Serialize> MapAccountDryRunResult<Balance> {
    /// Returns a result in json format
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

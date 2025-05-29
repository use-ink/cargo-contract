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

mod balance;
mod call;
mod contract_artifacts;
mod contract_info;
mod contract_storage;
mod env_check;
mod error;
pub mod events;
pub mod extrinsic_calls;
mod extrinsic_opts;
mod instantiate;
mod map_account;
pub mod pallet_revive_primitives;
mod remove;
mod rpc;
pub mod upload;

#[cfg(test)]
mod contract_storage_tests;

#[cfg(test)]
#[cfg(feature = "integration-tests")]
mod integration_tests;

use env_check::compare_node_env_with_contract;

use anyhow::Result;
pub use balance::{
    BalanceVariant,
    TokenMetadata,
};
pub use call::{
    CallCommandBuilder,
    CallExec,
};
pub use contract_artifacts::ContractArtifacts;
use contract_build::{
    CrateMetadata,
    Verbosity,
    DEFAULT_KEY_COL_WIDTH,
};
pub use contract_info::{
    fetch_all_contracts,
    fetch_contract_binary,
    fetch_contract_info,
    resolve_h160,
    ContractInfo,
    TrieId,
};
use contract_metadata::ContractMetadata;
pub use contract_storage::{
    ContractStorage,
    ContractStorageCell,
    ContractStorageLayout,
    ContractStorageRpc,
};
pub use contract_transcode::ContractMessageTranscoder;
pub use error::{
    ErrorVariant,
    GenericError,
};
pub use events::DisplayEvents;
pub use extrinsic_calls::{
    Call,
    Instantiate,
    InstantiateWithCode,
    UploadCode,
};
pub use extrinsic_opts::{
    ExtrinsicOpts,
    ExtrinsicOptsBuilder,
};
pub use instantiate::{
    contract_address,
    Code,
    InstantiateArgs,
    InstantiateCommandBuilder,
    InstantiateDryRunResult,
    InstantiateExec,
    InstantiateExecResult,
};
pub use map_account::{
    MapAccountCommandBuilder,
    MapAccountDryRunResult,
    MapAccountExec,
    MapAccountExecResult,
};
pub use remove::{
    RemoveCommandBuilder,
    RemoveExec,
};
use scale::{
    Decode,
    Encode,
};
use sp_core::{
    keccak_256,
    H160,
};
use subxt::{
    backend::legacy::LegacyRpcMethods,
    blocks,
    config::{
        DefaultExtrinsicParams,
        DefaultExtrinsicParamsBuilder,
        ExtrinsicParams,
        HashFor,
    },
    ext::subxt_rpcs::methods::legacy::DryRunResultBytes,
    tx,
    Config,
    OnlineClient,
};
pub use upload::{
    UploadCommandBuilder,
    UploadExec,
    UploadResult,
};

pub use rpc::{
    RawParams,
    RpcRequest,
};

/// The binary of a contract (compiled for PolkaVM).
#[derive(Debug, Clone)]
pub struct ContractBinary(Vec<u8>);

impl ContractBinary {
    /// The hash of the contract code: uniquely identifies the contract code on-chain.
    pub fn code_hash(&self) -> [u8; 32] {
        contract_build::code_hash(&self.0)
    }
}

/// Wait for the transaction to be included successfully into a block.
///
/// # Errors
///
/// If a runtime Module error occurs, this will only display the pallet and error indices.
/// Dynamic lookups of the actual error will be available once the following issue is
/// resolved: <https://github.com/paritytech/subxt/issues/443>.
///
/// # Finality
///
/// Currently, this will report success once the transaction is included in a block. In
/// the future there could be a flag to wait for finality before reporting success.
async fn submit_extrinsic<C, Call, Signer>(
    client: &OnlineClient<C>,
    rpc: &LegacyRpcMethods<C>,
    call: &Call,
    signer: &Signer,
) -> core::result::Result<blocks::ExtrinsicEvents<C>, subxt::Error>
where
    C: Config,
    Call: tx::Payload,
    Signer: tx::Signer<C>,
    <C::ExtrinsicParams as ExtrinsicParams<C>>::Params:
        From<<DefaultExtrinsicParams<C> as ExtrinsicParams<C>>::Params>,
{
    let account_id = Signer::account_id(signer);
    let account_nonce = get_account_nonce(client, rpc, &account_id).await?;

    let params = DefaultExtrinsicParamsBuilder::new()
        .nonce(account_nonce)
        .build();
    let mut tx = client
        .tx()
        .create_partial_offline(call, params.into())?
        .sign(signer)
        .submit_and_watch()
        .await?;

    // Below we use the low level API to replicate the `wait_for_in_block` behaviour which
    // was removed in subxt 0.33.0. See https://github.com/paritytech/subxt/pull/1237.
    //
    // We require this because we use `substrate-contracts-node` as our development node,
    // which does not currently support finality, so we just want to wait until it is
    // included in a block.
    use subxt::error::{
        RpcError,
        TransactionError,
    };
    use tx::TxStatus;

    while let Some(status) = tx.next().await {
        match status? {
            TxStatus::InBestBlock(tx_in_block)
            | TxStatus::InFinalizedBlock(tx_in_block) => {
                let events = tx_in_block.wait_for_success().await?;
                return Ok(events)
            }
            TxStatus::Error { message } => {
                return Err(TransactionError::Error(message).into())
            }
            TxStatus::Invalid { message } => {
                return Err(TransactionError::Invalid(message).into())
            }
            TxStatus::Dropped { message } => {
                return Err(TransactionError::Dropped(message).into())
            }
            _ => continue,
        }
    }
    Err(RpcError::SubscriptionDropped.into())
}

/// Wait for the transaction to be included successfully into a block. Returns the
/// estimated fee to execute the transaction.
///
/// # Errors
///
/// If a runtime Module error occurs, this will only display the pallet and error indices.
/// Dynamic lookups of the actual error will be available once the following issue is
/// resolved: <https://github.com/paritytech/subxt/issues/443>.
///
/// # Finality
///
/// Currently this will report success once the transaction is included in a block. In the
/// future there could be a flag to wait for finality before reporting success.
async fn dry_run_extrinsic<C, Call, Signer>(
    client: &OnlineClient<C>,
    rpc: &LegacyRpcMethods<C>,
    call: &Call,
    signer: &Signer,
) -> core::result::Result<(DryRunResultBytes, u128), subxt::Error>
where
    C: Config,
    Call: tx::Payload,
    Signer: tx::Signer<C>,
    <C::ExtrinsicParams as ExtrinsicParams<C>>::Params:
        From<<DefaultExtrinsicParams<C> as ExtrinsicParams<C>>::Params>,
{
    let account_id = Signer::account_id(signer);
    let account_nonce = get_account_nonce(client, rpc, &account_id).await?;

    let params = DefaultExtrinsicParamsBuilder::new()
        .nonce(account_nonce)
        .build();
    let extrinsic = client
        .tx()
        .create_partial_offline(call, params.into())?
        .sign(signer);
    let result = rpc.dry_run(extrinsic.encoded(), None).await?;
    let partial_fee_estimate = extrinsic.partial_fee_estimate().await?;
    Ok((result, partial_fee_estimate))
}

/// Return the account nonce at the *best* block for an account ID.
async fn get_account_nonce<C>(
    client: &OnlineClient<C>,
    rpc: &LegacyRpcMethods<C>,
    account_id: &C::AccountId,
) -> core::result::Result<u64, subxt::Error>
where
    C: Config,
{
    let best_block = rpc
        .chain_get_block_hash(None)
        .await?
        .ok_or(subxt::Error::Other("Best block not found".into()))?;
    let account_nonce = client
        .blocks()
        .at(best_block)
        .await?
        .account_nonce(account_id)
        .await?;
    Ok(account_nonce)
}

async fn state_call<C, A: Encode, R: Decode>(
    rpc: &LegacyRpcMethods<C>,
    func: &str,
    args: A,
) -> Result<R>
where
    C: Config,
{
    let params = args.encode();
    let bytes = rpc.state_call(func, Some(&params), None).await?;
    Ok(R::decode(&mut bytes.as_ref())?)
}

/// Fetch the hash of the *best* block (included but not guaranteed to be finalized).
async fn get_best_block<C>(
    rpc: &LegacyRpcMethods<C>,
) -> core::result::Result<HashFor<C>, subxt::Error>
where
    C: Config,
{
    rpc.chain_get_block_hash(None)
        .await?
        .ok_or(subxt::Error::Other("Best block not found".into()))
}

fn check_env_types<C>(
    client: &OnlineClient<C>,
    transcoder: &ContractMessageTranscoder,
    verbosity: &Verbosity,
) -> Result<()>
where
    C: Config,
{
    compare_node_env_with_contract(
        client.metadata().types(),
        transcoder.metadata(),
        verbosity,
    )
}

// Converts a Url into a String representation without excluding the default port.
pub fn url_to_string(url: &url::Url) -> String {
    match (url.port(), url.port_or_known_default()) {
        (None, Some(port)) => {
            format!(
                "{}:{port}{}",
                &url[..url::Position::AfterHost],
                &url[url::Position::BeforePath..]
            )
            .to_string()
        }
        _ => url.to_string(),
    }
}

pub struct AccountIdMapper {}
impl AccountIdMapper {
    //pub fn to_address(account_id: &E::AccountId) -> H160 {
    pub fn to_address(account_id: &[u8]) -> H160 {
        let mut account_bytes: [u8; 32] = [0u8; 32];
        account_bytes.copy_from_slice(&account_id[..32]);
        if Self::is_eth_derived(account_id) {
            // this was originally an eth address
            // we just strip the 0xEE suffix to get the original address
            H160::from_slice(&account_bytes[..20])
        } else {
            // this is an (ed|sr)25510 derived address
            // avoid truncating the public key by hashing it first
            let account_hash = keccak_256(account_bytes.as_ref());
            H160::from_slice(&account_hash[12..])
        }
    }

    /// Returns true if the passed account id is controlled by an Ethereum key.
    ///
    /// This is a stateless check that just compares the last 12 bytes. Please note that
    /// it is theoretically possible to create an ed25519 keypair that passed this
    /// filter. However, this can't be used for an attack. It also won't happen by
    /// accident since everbody is using sr25519 where this is not a valid public key.
    //fn is_eth_derived(account_id: &[u8]) -> bool {
    fn is_eth_derived(account_bytes: &[u8]) -> bool {
        account_bytes[20..] == [0xEE; 12]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_to_string_works() {
        // with custom port
        let url = url::Url::parse("ws://127.0.0.1:9944").unwrap();
        assert_eq!(url_to_string(&url), "ws://127.0.0.1:9944/");

        // with default port
        let url = url::Url::parse("wss://127.0.0.1:443").unwrap();
        assert_eq!(url_to_string(&url), "wss://127.0.0.1:443/");

        // with default port and path
        let url = url::Url::parse("wss://127.0.0.1:443/test/1").unwrap();
        assert_eq!(url_to_string(&url), "wss://127.0.0.1:443/test/1");

        // with default port and domain
        let url = url::Url::parse("wss://test.io:443").unwrap();
        assert_eq!(url_to_string(&url), "wss://test.io:443/");

        // with default port, domain and path
        let url = url::Url::parse("wss://test.io/test/1").unwrap();
        assert_eq!(url_to_string(&url), "wss://test.io:443/test/1");
    }
}

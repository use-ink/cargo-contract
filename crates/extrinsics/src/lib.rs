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

mod balance;
mod call;
mod contract_artifacts;
mod contract_info;
mod env_check;
mod error;
mod events;
mod extrinsic_opts;
mod instantiate;
mod remove;
mod runtime_api;
mod upload;

#[cfg(test)]
#[cfg(feature = "integration-tests")]
mod integration_tests;

use env_check::compare_node_env_with_contract;

use anyhow::Result;
use contract_build::{
    CrateMetadata,
    DEFAULT_KEY_COL_WIDTH,
};
use scale::{
    Decode,
    Encode,
};
use subxt::{
    backend::legacy::LegacyRpcMethods,
    blocks,
    config,
    tx,
    utils::AccountId32,
    Config,
    OnlineClient,
};
use subxt_signer::sr25519::Keypair;

pub use balance::{
    BalanceVariant,
    TokenMetadata,
};
pub use call::{
    CallCommandBuilder,
    CallExec,
    CallRequest,
};
pub use contract_artifacts::ContractArtifacts;
pub use contract_info::{
    ContractInfo,
    ContractInfoRpc,
    ContractStorageKey,
};
use contract_metadata::ContractMetadata;
pub use contract_transcode::ContractMessageTranscoder;
pub use error::{
    ErrorVariant,
    GenericError,
};
pub use events::DisplayEvents;
pub use extrinsic_opts::{
    state,
    ExtrinsicOptsBuilder,
    Missing,
};
pub use instantiate::{
    Code,
    InstantiateArgs,
    InstantiateCommandBuilder,
    InstantiateDryRunResult,
    InstantiateExec,
    InstantiateExecResult,
};
pub use remove::{
    RemoveCommandBuilder,
    RemoveExec,
};

pub use subxt::PolkadotConfig as DefaultConfig;
pub use upload::{
    CodeUploadRequest,
    UploadCommandBuilder,
    UploadExec,
    UploadResult,
};

pub type Client = OnlineClient<DefaultConfig>;
pub type Balance = u128;
pub type CodeHash = <DefaultConfig as Config>::Hash;

/// The Wasm code of a contract.
#[derive(Debug)]
pub struct WasmCode(Vec<u8>);

impl WasmCode {
    /// The hash of the contract code: uniquely identifies the contract code on-chain.
    pub fn code_hash(&self) -> [u8; 32] {
        contract_build::code_hash(&self.0)
    }
}

/// Get the account id from the Keypair
pub fn account_id(keypair: &Keypair) -> AccountId32 {
    subxt::tx::Signer::<DefaultConfig>::account_id(keypair)
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
/// Currently this will report success once the transaction is included in a block. In the
/// future there could be a flag to wait for finality before reporting success.
async fn submit_extrinsic<T, Call, Signer>(
    client: &OnlineClient<T>,
    rpc: &LegacyRpcMethods<T>,
    call: &Call,
    signer: &Signer,
) -> core::result::Result<blocks::ExtrinsicEvents<T>, subxt::Error>
where
    T: Config,
    Call: tx::TxPayload,
    Signer: tx::Signer<T>,
    <T::ExtrinsicParams as config::ExtrinsicParams<T>>::OtherParams: Default,
{
    let account_id = Signer::account_id(signer);
    let account_nonce = get_account_nonce(client, rpc, &account_id).await?;

    client
        .tx()
        .create_signed_with_nonce(call, signer, account_nonce, Default::default())?
        .submit_and_watch()
        .await?
        .wait_for_in_block()
        .await?
        .wait_for_success()
        .await
}

/// Return the account nonce at the *best* block for an account ID.
async fn get_account_nonce<T>(
    client: &OnlineClient<T>,
    rpc: &LegacyRpcMethods<T>,
    account_id: &T::AccountId,
) -> core::result::Result<u64, subxt::Error>
where
    T: Config,
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

async fn state_call<A: Encode, R: Decode>(
    rpc: &LegacyRpcMethods<DefaultConfig>,
    func: &str,
    args: A,
) -> Result<R> {
    let params = args.encode();
    let bytes = rpc.state_call(func, Some(&params), None).await?;
    Ok(R::decode(&mut bytes.as_ref())?)
}

/// Parse a hex encoded 32 byte hash. Returns error if not exactly 32 bytes.
pub fn parse_code_hash(input: &str) -> Result<<DefaultConfig as Config>::Hash> {
    let bytes = contract_build::util::decode_hex(input)?;
    if bytes.len() != 32 {
        anyhow::bail!("Code hash should be 32 bytes in length")
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr.into())
}

/// Fetch the hash of the *best* block (included but not guaranteed to be finalized).
async fn get_best_block(
    rpc: &LegacyRpcMethods<DefaultConfig>,
) -> core::result::Result<<DefaultConfig as Config>::Hash, subxt::Error> {
    rpc.chain_get_block_hash(None)
        .await?
        .ok_or(subxt::Error::Other("Best block not found".into()))
}

fn check_env_types<T>(
    client: &OnlineClient<T>,
    transcoder: &ContractMessageTranscoder,
) -> Result<()>
where
    T: Config,
{
    compare_node_env_with_contract(client.metadata().types(), transcoder.metadata())
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

/// Copy of `pallet_contracts_primitives::StorageDeposit` which implements `Serialize`,
/// required for json output.
#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, serde::Serialize)]
pub enum StorageDeposit {
    /// The transaction reduced storage consumption.
    ///
    /// This means that the specified amount of balance was transferred from the involved
    /// contracts to the call origin.
    Refund(Balance),
    /// The transaction increased overall storage usage.
    ///
    /// This means that the specified amount of balance was transferred from the call
    /// origin to the contracts involved.
    Charge(Balance),
}

impl From<&pallet_contracts_primitives::StorageDeposit<Balance>> for StorageDeposit {
    fn from(deposit: &pallet_contracts_primitives::StorageDeposit<Balance>) -> Self {
        match deposit {
            pallet_contracts_primitives::StorageDeposit::Refund(balance) => {
                Self::Refund(*balance)
            }
            pallet_contracts_primitives::StorageDeposit::Charge(balance) => {
                Self::Charge(*balance)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_code_hash_works() {
        // with 0x prefix
        assert!(parse_code_hash(
            "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
        )
        .is_ok());
        // without 0x prefix
        assert!(parse_code_hash(
            "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
        )
        .is_ok())
    }

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

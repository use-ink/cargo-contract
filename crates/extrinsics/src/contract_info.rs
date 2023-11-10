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

use anyhow::{
    anyhow,
    Result,
};

use super::{
    get_best_block,
    runtime_api::api,
    url_to_string,
    Balance,
    Client,
    CodeHash,
    DefaultConfig,
};

use scale::Decode;
use sp_core::storage::PrefixedStorageKey;
use std::option::Option;
use subxt::{
    backend::{
        legacy::{
            rpc_methods::Bytes,
            LegacyRpcMethods,
        },
        rpc::{
            rpc_params,
            RpcClient,
        },
    },
    utils::AccountId32,
    Config,
    OnlineClient,
};

/// Methods for querying contracts over RPC.
pub struct ContractInfoRpc {
    rpc_client: RpcClient,
    rpc_methods: LegacyRpcMethods<DefaultConfig>,
    client: Client,
}

impl ContractInfoRpc {
    /// Create a new instance of the ContractsRpc.
    pub async fn new(url: &url::Url) -> Result<Self> {
        let rpc_client = RpcClient::from_url(url_to_string(&url)).await?;
        let client =
            OnlineClient::<DefaultConfig>::from_rpc_client(rpc_client.clone()).await?;
        let rpc_methods = LegacyRpcMethods::<DefaultConfig>::new(rpc_client.clone());

        Ok(Self {
            rpc_client,
            rpc_methods,
            client,
        })
    }

    /// Fetch the contract info from the storage using the provided client.
    pub async fn fetch_contract_info(
        &self,
        contract: &AccountId32,
    ) -> Result<Option<ContractInfo>> {
        let info_contract_call = api::storage().contracts().contract_info_of(contract);

        let best_block = get_best_block(&self.rpc_methods).await?;

        let contract_info_of = self
            .client
            .storage()
            .at(best_block)
            .fetch(&info_contract_call)
            .await?;

        match contract_info_of {
            Some(info_result) => {
                let convert_trie_id = hex::encode(info_result.trie_id.0);
                Ok(Some(ContractInfo {
                    trie_id: convert_trie_id,
                    code_hash: info_result.code_hash,
                    storage_items: info_result.storage_items,
                    storage_item_deposit: info_result.storage_item_deposit,
                }))
            }
            None => Ok(None),
        }
    }

    /// Fetch the contract storage at the given key.
    ///
    /// For more information about how storage keys are calculated see: https://use.ink/datastructures/storage-in-metadata
    pub async fn fetch_contract_storage(
        &self,
        child_storage_key: &PrefixedStorageKey,
        key: &ContractStorageKey,
        block_hash: Option<<DefaultConfig as Config>::Hash>,
    ) -> Result<Option<Vec<u8>>> {
        let key_hex = key.hashed_to_hex();
        tracing::debug!("fetch_contract_storage: child_storage_key: {child_storage_key:?} for key: {key_hex:?}");
        let params = rpc_params![child_storage_key, key_hex, block_hash];
        let data: Option<Bytes> = self
            .rpc_client
            .request("childstate_getStorage", params)
            .await?;
        Ok(data.map(|b| b.0))
    }

    pub async fn fetch_storage_keys_paged(
        &self,
        child_storage_key: &PrefixedStorageKey,
        prefix: Option<&[u8]>,
        count: u32,
        start_key: Option<&[u8]>,
        block_hash: Option<<DefaultConfig as Config>::Hash>,
    ) -> Result<Vec<Vec<u8>>> {
        let prefix_hex = prefix.map(|p| format!("0x{}", hex::encode(p)));
        let start_key_hex = start_key.map(|p| format!("0x{}", hex::encode(p)));
        let params = rpc_params![
            child_storage_key,
            prefix_hex,
            count,
            start_key_hex,
            block_hash
        ];
        let data: Vec<Bytes> = self
            .rpc_client
            .request("childstate_getKeysPaged", params)
            .await?;
        Ok(data.into_iter().map(|b| b.0).collect())
    }

    /// Fetch the contract wasm code from the storage using the provided client and code
    /// hash.
    pub async fn fetch_wasm_code(&self, hash: &CodeHash) -> Result<Option<Vec<u8>>> {
        let pristine_code_address = api::storage().contracts().pristine_code(hash);
        let best_block = get_best_block(&self.rpc_methods).await?;

        let pristine_bytes = self
            .client
            .storage()
            .at(best_block)
            .fetch(&pristine_code_address)
            .await?
            .map(|v| v.0);

        Ok(pristine_bytes)
    }

    /// Fetch all contract addresses from the storage using the provided client and count
    /// of requested elements starting from an optional address
    pub async fn fetch_all_contracts(&self) -> Result<Vec<AccountId32>> {
        let root_key = api::storage()
            .contracts()
            .contract_info_of_iter()
            .to_root_bytes();

        let best_block = get_best_block(&self.rpc_methods).await?;
        let mut keys = self
            .client
            .storage()
            .at(best_block)
            .fetch_raw_keys(root_key.clone())
            .await?;

        let mut contract_accounts = Vec::new();
        while let Some(result) = keys.next().await {
            let key = result?;
            let contract_account = parse_contract_account_address(&key, root_key.len())?;
            contract_accounts.push(contract_account);
        }

        Ok(contract_accounts)
    }
}

#[derive(serde::Serialize)]
pub struct ContractInfo {
    trie_id: String,
    code_hash: CodeHash,
    storage_items: u32,
    storage_item_deposit: Balance,
}

impl ContractInfo {
    /// Convert and return contract info in JSON format.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Return the trie_id of the contract.
    pub fn trie_id(&self) -> &str {
        &self.trie_id
    }

    /// Return the code_hash of the contract.
    pub fn code_hash(&self) -> &CodeHash {
        &self.code_hash
    }

    /// Return the number of storage items of the contract.
    pub fn storage_items(&self) -> u32 {
        self.storage_items
    }

    /// Return the storage item deposit of the contract.
    pub fn storage_item_deposit(&self) -> Balance {
        self.storage_item_deposit
    }

    /// Get the prefixed storage key for the contract, used to access the contract's
    /// storage
    pub fn prefixed_storage_key(&self) -> PrefixedStorageKey {
        let trie_id = hex::decode(&self.trie_id)
            .expect("trie_id should be valid hex encoded bytes.");
        sp_core::storage::ChildInfo::new_default(&trie_id).into_prefixed_storage_key()
    }
}

/// Parse a contract account address from a storage key. Returns error if a key is
/// malformed.
fn parse_contract_account_address(
    storage_contract_account_key: &[u8],
    storage_contract_root_key_len: usize,
) -> Result<AccountId32> {
    // storage_contract_account_key is a concatenation of contract_info_of root key and
    // Twox64Concat(AccountId)
    let mut account = storage_contract_account_key
        .get(storage_contract_root_key_len + 8..)
        .ok_or(anyhow!("Unexpected storage key size"))?;
    AccountId32::decode(&mut account)
        .map_err(|err| anyhow!("AccountId deserialization error: {}", err))
}

/// Represents a 32 bit storage key within a contract's storage.
pub struct ContractStorageKey {
    raw: [u8; 4],
}

impl ContractStorageKey {
    /// Create a new instance of the ContractStorageKey.
    pub fn new(raw: [u8; 4]) -> Self {
        Self { raw }
    }

    /// Returns the hex encoded hashed `blake2_128_concat` representation of the storage
    /// key.
    pub fn hashed_to_hex(&self) -> String {
        use blake2::digest::{
            consts::U16,
            Digest as _,
        };

        let mut blake2_128 = blake2::Blake2b::<U16>::new();
        blake2_128.update(&self.raw);
        let result = blake2_128.finalize();

        let concat = result
            .as_slice()
            .iter()
            .chain(self.raw.iter())
            .cloned()
            .collect::<Vec<_>>();

        hex::encode(concat)
    }
}

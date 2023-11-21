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

use anyhow::Result;
use ink_metadata::{
    layout::{
        Layout,
        LayoutKey,
    },
    InkProject,
};
use sp_core::storage::ChildInfo;
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

use super::{
    fetch_contract_info,
    url_to_string,
    Client,
    ContractInfo,
    DefaultConfig,
    TrieId,
};

pub struct ContractStorageLayout {
    metadata: InkProject,
    root_key: ContractStorageKey,
}

impl ContractStorageLayout {
    pub fn new(metadata: InkProject) -> Result<Self> {
        if let Layout::Root(root) = metadata.layout() {
            let root_key = ContractStorageKey::from(root.root_key());
            Ok(Self { metadata, root_key })
        } else {
            Err(anyhow::anyhow!("No root layout found in metadata"))
        }
    }
    pub fn root_key(&self) -> &ContractStorageKey {
        &self.root_key
    }
}

impl TryFrom<contract_metadata::ContractMetadata> for ContractStorageLayout {
    type Error = anyhow::Error;

    fn try_from(
        metadata: contract_metadata::ContractMetadata,
    ) -> Result<Self, Self::Error> {
        let ink_project =
            serde_json::from_value(serde_json::Value::Object(metadata.abi))?;
        Self::new(ink_project)
    }
}

/// Methods for querying contracts over RPC.
pub struct ContractStorageRpc {
    rpc_client: RpcClient,
    rpc_methods: LegacyRpcMethods<DefaultConfig>,
    client: Client,
}

impl ContractStorageRpc {
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

    /// Fetch the contract info to access the trie id for querying storage.
    pub async fn fetch_contract_info(
        &self,
        contract: &AccountId32,
    ) -> Result<ContractInfo> {
        fetch_contract_info(contract, &self.rpc_methods, &self.client).await
    }

    /// Fetch the contract storage at the given key.
    ///
    /// For more information about how storage keys are calculated see: https://use.ink/datastructures/storage-in-metadata
    pub async fn fetch_contract_storage(
        &self,
        trie_id: &TrieId,
        key: &ContractStorageKey,
        block_hash: Option<<DefaultConfig as Config>::Hash>,
    ) -> Result<Option<Vec<u8>>> {
        let child_storage_key =
            ChildInfo::new_default(trie_id.as_ref()).into_prefixed_storage_key();
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
        trie_id: &TrieId,
        prefix: Option<&[u8]>,
        count: u32,
        start_key: Option<&[u8]>,
        block_hash: Option<<DefaultConfig as Config>::Hash>,
    ) -> Result<Vec<Vec<u8>>> {
        let child_storage_key =
            ChildInfo::new_default(trie_id.as_ref()).into_prefixed_storage_key();
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
}

/// Represents a 32 bit storage key within a contract's storage.
pub struct ContractStorageKey {
    raw: u32,
}

impl From<&LayoutKey> for ContractStorageKey {
    fn from(key: &LayoutKey) -> Self {
        Self { raw: *key.key() }
    }
}

impl ContractStorageKey {
    /// Create a new instance of the ContractStorageKey.
    pub fn new(raw: u32) -> Self {
        Self { raw }
    }

    pub fn bytes(&self) -> [u8; 4] {
        self.raw.to_be_bytes()
    }

    /// Returns the hex encoded hashed `blake2_128_concat` representation of the storage
    /// key.
    pub fn hashed_to_hex(&self) -> String {
        use blake2::digest::{
            consts::U16,
            Digest as _,
        };

        let mut blake2_128 = blake2::Blake2b::<U16>::new();
        blake2_128.update(&self.bytes());
        let result = blake2_128.finalize();

        let concat = result
            .as_slice()
            .iter()
            .chain(self.bytes().iter())
            .cloned()
            .collect::<Vec<_>>();

        hex::encode(concat)
    }
}

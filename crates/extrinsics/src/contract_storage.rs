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
use contract_metadata::byte_str;
use ink_metadata::{
    layout::Layout,
    InkProject,
};
use scale_info::form::PortableForm;
use serde::Serialize;
use sp_core::storage::ChildInfo;
use std::{
    collections::BTreeMap,
    fmt::Display,
};
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
        BlockRef,
    },
    error::DecodeError,
    ext::scale_decode::{
        IntoVisitor,
        Visitor,
    },
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

pub struct ContractStorage<C: Config = DefaultConfig> {
    metadata: InkProject,
    rpc: ContractStorageRpc<C>,
}

impl<C: Config> ContractStorage<C>
where
    C::AccountId: AsRef<[u8]> + Display + IntoVisitor,
    DecodeError: From<<<C::AccountId as IntoVisitor>::Visitor as Visitor>::Error>,
    BlockRef<sp_core::H256>: From<C::Hash>,
{
    pub fn new(metadata: InkProject, rpc: ContractStorageRpc<C>) -> Self {
        Self { metadata, rpc }
    }

    /// Load the raw key/value storage for a given contract.
    pub async fn load_contract_storage_data(
        &self,
        contract_account: &C::AccountId,
    ) -> Result<ContractStorageData> {
        let contract_info = self.rpc.fetch_contract_info(&contract_account).await?;
        let trie_id = contract_info.trie_id();

        let storage_keys = self
            .rpc
            .fetch_storage_keys_paged(trie_id, None, 1000, None, None) // todo loop pages
            .await?;
        let storage_values = self
            .rpc
            .fetch_storage_entries(trie_id, &storage_keys, None)
            .await?;
        assert_eq!(
            storage_keys.len(),
            storage_values.len(),
            "storage keys and values must be the same length"
        );
        let storage = storage_keys
            .into_iter()
            .zip(storage_values.into_iter())
            .filter_map(|(key, value)| value.map(|v| (key, v)))
            .collect();

        let contract_storage = ContractStorageData(storage);
        Ok(contract_storage)
    }

    pub async fn load_contract_storage_with_layout(
        &self,
        contract_account: &C::AccountId,
    ) -> Result<ContractStorageLayout> {
        let data = self.load_contract_storage_data(contract_account).await?;
        let layout = ContractStorageLayout::new(data, self.metadata.layout());
        Ok(layout)
    }
}

/// Represents the raw key/value storage for the contract.
#[derive(Serialize)]
pub struct ContractStorageData(BTreeMap<Bytes, Bytes>);

#[derive(Serialize)]
pub struct ContractStorageLayout {
    cells: Vec<ContractStorageCell>,
}

impl ContractStorageLayout {
    pub fn new(data: ContractStorageData, layout: &Layout<PortableForm>) -> Self {
        let mut root_keys = Vec::new();
        Self::collect_roots("root".to_string(), layout, &mut root_keys);

        let cells = data
            .0
            .iter()
            .filter_map(|(k, v)| {
                assert!(k.0.len() >= 20, "key must be at least 20 bytes");
                let root_key = {
                    let mut key = [0u8; 4];
                    key.copy_from_slice(&k.0[16..20]);
                };
                let mapping_key = if k.0.len() > 20 {
                    Some(Bytes::from(k.0[20..].to_vec()))
                } else {
                    None
                };

                let root = root_keys.iter().find(|(_, key)| key.0 == &root_key.to_string()).unwrap();

                if root_key != *root.root_key().key() {
                    None
                } else {
                    Some(ContractStorageCell {
                        key: k.clone(),
                        value: v.clone(),
                        root_key,
                        mapping_key,
                        label: label.clone(),
                    })
                }
            })
            .collect();

        Self { cells }
    }

    fn collect_root_keys(
        label: String,
        layout: &Layout<PortableForm>,
        root_keys: &mut Vec<(String, Bytes)>,
    ) {
        match layout {
            Layout::Root(root) => {

                root_keys.append(&mut cells);
                Self::collect_roots(label, root.layout(), root_keys)
            }
            Layout::Struct(struct_layout) => {
                for field in struct_layout.fields() {
                    let label = field.name().to_string();
                    println!("field: {}", label);
                    Self::collect_roots(label, field.layout(), root_keys);
                }
            }
            Layout::Enum(enum_layout) => {
                for (variant, struct_layout) in enum_layout.variants() {
                    for field in struct_layout.fields() {
                        let label =
                            format!("{}::{}", enum_layout.name(), variant.value());
                        Self::collect_roots(label, field.layout(), root_keys);
                    }
                }
            }
            Layout::Array(_) => {
                todo!("Figure out what to do with an array layout")
            }
            Layout::Hash(_) => {
                unimplemented!("Layout::Hash is not currently be constructed")
            }
            Layout::Leaf(_) => {}
        }
    }
}

#[derive(Serialize)]
pub struct ContractStorageCell {
    key: Bytes,
    value: Bytes,
    #[serde(serialize_with = "byte_str::serialize_as_byte_str")]
    root_key: [u8; 4],
    mapping_key: Option<Bytes>,
    label: String,
}

/// Methods for querying contracts over RPC.
pub struct ContractStorageRpc<C: Config> {
    rpc_client: RpcClient,
    rpc_methods: LegacyRpcMethods<C>,
    client: Client,
}

impl<C: Config> ContractStorageRpc<C>
where
    C::AccountId: AsRef<[u8]> + Display + IntoVisitor,
    DecodeError: From<<<C::AccountId as IntoVisitor>::Visitor as Visitor>::Error>,
    BlockRef<sp_core::H256>: From<C::Hash>,
{
    /// Create a new instance of the ContractsRpc.
    pub async fn new(url: &url::Url) -> Result<Self> {
        let rpc_client = RpcClient::from_url(url_to_string(&url)).await?;
        let client = OnlineClient::from_rpc_client(rpc_client.clone()).await?;
        let rpc_methods = LegacyRpcMethods::new(rpc_client.clone());

        Ok(Self {
            rpc_client,
            rpc_methods,
            client,
        })
    }

    /// Fetch the contract info to access the trie id for querying storage.
    pub async fn fetch_contract_info(
        &self,
        contract: &C::AccountId,
    ) -> Result<ContractInfo> {
        fetch_contract_info(contract, &self.rpc_methods, &self.client).await
    }

    /// Fetch the contract storage at the given key.
    ///
    /// For more information about how storage keys are calculated see: https://use.ink/datastructures/storage-in-metadata
    pub async fn fetch_contract_storage(
        &self,
        trie_id: &TrieId,
        key: &Bytes,
        block_hash: Option<C::Hash>,
    ) -> Result<Option<Bytes>> {
        let child_storage_key =
            ChildInfo::new_default(trie_id.as_ref()).into_prefixed_storage_key();
        let params = rpc_params![child_storage_key, key, block_hash];
        let data: Option<Bytes> = self
            .rpc_client
            .request("childstate_getStorage", params)
            .await?;
        Ok(data)
    }

    pub async fn fetch_storage_keys_paged(
        &self,
        trie_id: &TrieId,
        prefix: Option<&[u8]>,
        count: u32,
        start_key: Option<&[u8]>,
        block_hash: Option<C::Hash>,
    ) -> Result<Vec<Bytes>> {
        let child_storage_key =
            ChildInfo::new_default(trie_id.as_ref()).into_prefixed_storage_key();
        let prefix_hex = prefix.map(|p| format!("0x{}", hex::encode(p)));
        let start_key_hex = start_key.map(|k| format!("0x{}", hex::encode(k)));
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
        Ok(data)
    }

    pub async fn fetch_storage_entries(
        &self,
        trie_id: &TrieId,
        keys: &[Bytes],
        block_hash: Option<C::Hash>,
    ) -> Result<Vec<Option<Bytes>>> {
        let child_storage_key =
            ChildInfo::new_default(trie_id.as_ref()).into_prefixed_storage_key();
        let params = rpc_params![child_storage_key, keys, block_hash];
        let data: Vec<Option<Bytes>> = self
            .rpc_client
            .request("childstate_getStorageEntries", params)
            .await?;
        Ok(data)
    }
}

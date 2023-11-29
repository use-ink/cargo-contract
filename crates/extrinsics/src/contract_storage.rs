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
        StructLayout,
    },
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
    ContractInfo,
    DefaultConfig,
    TrieId,
};

pub struct ContractStorage<C: Config = DefaultConfig> {
    rpc: ContractStorageRpc<C>,
}

impl<C: Config> ContractStorage<C>
where
    C::AccountId: AsRef<[u8]> + Display + IntoVisitor,
    DecodeError: From<<<C::AccountId as IntoVisitor>::Visitor as Visitor>::Error>,
{
    pub fn new(rpc: ContractStorageRpc<C>) -> Self {
        Self { rpc }
    }

    /// Load the raw key/value storage for a given contract.
    pub async fn load_contract_storage_data(
        &self,
        contract_account: &C::AccountId,
    ) -> Result<ContractStorageData> {
        let contract_info = self.rpc.fetch_contract_info(contract_account).await?;
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
        metadata: &InkProject,
        contract_account: &C::AccountId,
    ) -> Result<ContractStorageLayout> {
        let data = self.load_contract_storage_data(contract_account).await?;
        let layout = ContractStorageLayout::new(data, metadata.layout());
        Ok(layout)
    }
}

/// Represents the raw key/value storage for the contract.
#[derive(Serialize)]
pub struct ContractStorageData(BTreeMap<Bytes, Bytes>);

#[derive(Serialize)]
pub struct ContractStorageCell {
    pub path: Vec<String>,
    pub value: Bytes,
    pub type_id: u32,
    pub root_key: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapping_key: Option<Bytes>,
}

#[derive(Serialize)]
pub struct ContractStorageLayout(pub BTreeMap<Bytes, ContractStorageCell>);

impl ContractStorageLayout {
    pub fn new(data: ContractStorageData, layout: &Layout<PortableForm>) -> Self {
        let mut path_stack = vec!["root".to_string()];
        let mut root_keys: std::vec::Vec<(_, _, _)> = Vec::new();
        Self::collect_root_key_entries(layout, &mut path_stack, &mut root_keys);

        let cells = data
            .0
            .iter()
            .filter_map(|(k, v)| {
                let (root_key, mapping_key) = Self::key_parts(k);
                let (path, _, type_id) =
                    root_keys.iter().find(|(_, key, _)| *key == root_key)?;

                Some((
                    k.clone(),
                    ContractStorageCell {
                        path: path.clone(),
                        value: v.clone(),
                        type_id: *type_id,
                        root_key,
                        mapping_key,
                    },
                ))
            })
            .collect();
        Self(cells)
    }

    fn collect_root_key_entries(
        layout: &Layout<PortableForm>,
        path: &mut Vec<String>,
        data: &mut Vec<(Vec<String>, u32, u32)>,
    ) {
        match layout {
            Layout::Root(root) => {
                data.push((path.clone(), *root.root_key().key(), root.ty().id));
                Self::collect_root_key_entries(root.layout(), path, data);
            }
            Layout::Struct(struct_layout) => {
                Self::struct_entries(struct_layout, path, data)
            }
            Layout::Enum(enum_layout) => {
                path.push(enum_layout.name().to_string());
                for (variant, struct_layout) in enum_layout.variants() {
                    path.push(variant.value().to_string());
                    Self::struct_entries(struct_layout, path, data);
                    path.pop();
                }
                path.pop();
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

    fn struct_entries(
        struct_layout: &StructLayout<PortableForm>,
        path: &mut Vec<String>,
        data: &mut Vec<(Vec<String>, u32, u32)>,
    ) {
        let struct_label = struct_layout.name().to_string();
        path.push(struct_label);
        for field in struct_layout.fields() {
            path.push(field.name().to_string());
            Self::collect_root_key_entries(field.layout(), path, data);
            path.pop();
        }
        path.pop();
    }

    /// Split the key up
    ///
    /// 0x6a3fa479de3b1efe271333d8974501c8e7dc23266dd9bfa5543a94aad824cfb29396d200926d28223c57df8954cf0dc16812ea47
    /// |--------------------------------|---------|-------------------------------------------------------------|
    ///       blake2_128 of raw key        root key                         mapping key
    fn key_parts(key: &Bytes) -> (u32, Option<Bytes>) {
        assert!(key.0.len() >= 20, "key must be at least 20 bytes");
        let mut root_key_bytes = [0u8; 4];
        root_key_bytes.copy_from_slice(&key.0[16..20]);

        // keys are SCALE encoded (little endian), so the root key
        let root_key = <u32 as scale::Decode>::decode(&mut &root_key_bytes[..])
            .expect("root key is 4 bytes, it always decodes successfully to a u32; qed");

        let mapping_key = if key.0.len() > 20 {
            Some(Bytes::from(key.0[20..].to_vec()))
        } else {
            None
        };

        (root_key, mapping_key)
    }
}

/// Methods for querying contracts over RPC.
pub struct ContractStorageRpc<C: Config> {
    rpc_client: RpcClient,
    rpc_methods: LegacyRpcMethods<C>,
    client: OnlineClient<C>,
}

impl<C: Config> ContractStorageRpc<C>
where
    C::AccountId: AsRef<[u8]> + Display + IntoVisitor,
    DecodeError: From<<<C::AccountId as IntoVisitor>::Visitor as Visitor>::Error>,
    // BlockRef<sp_core::H256>: From<C::Hash>,
{
    /// Create a new instance of the ContractsRpc.
    pub async fn new(url: &url::Url) -> Result<Self> {
        let rpc_client = RpcClient::from_url(url_to_string(url)).await?;
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

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
use contract_transcode::ContractMessageTranscoder;
use ink_metadata::{
    layout::{
        Layout,
        StructLayout,
    },
    InkProject,
};
use itertools::Itertools;
use scale::Decode;
use scale_info::{
    form::PortableForm,
    Type,
};
use serde::{Serialize, Serializer};
use sp_core::{
    hexdisplay::AsBytesRef,
    storage::ChildInfo,
};
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

        let mut storage_keys = Vec::new();
        const KEYS_COUNT: u32 = 1000;
        loop {
            let mut keys = self
                .rpc
                .fetch_storage_keys_paged(
                    trie_id,
                    None,
                    KEYS_COUNT,
                    storage_keys.last().map(|k: &Bytes| k.as_bytes_ref()),
                    None,
                )
                .await?;
            let count = keys.len();
            storage_keys.append(&mut keys);
            if (count as u32) < KEYS_COUNT {
                break
            }
        }
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
        let layout = ContractStorageLayout::new(data, metadata);
        Ok(layout)
    }
}

fn key_as_hex<S>(key: &u32, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer
{
    serializer.serialize_str(format!("0x{}", hex::encode(key.to_le_bytes())).as_str())
}

/// Represents the raw key/value storage for the contract.
#[derive(Serialize, Debug)]
pub struct ContractStorageData(BTreeMap<Bytes, Bytes>);

#[derive(Serialize, Debug)]
pub struct RootKeyEntry {
    #[serde(serialize_with = "key_as_hex")]
    pub root_key: u32,
    pub path: Vec<String>,
    pub type_id: u32,
}

trait DecodePrettyString {
    /// Decode the storage cell into a multiline string
    fn decode_pretty_string(&self, decoder: &ContractMessageTranscoder) -> String;

    fn not_decoded_hex_bytes(value: &Bytes) -> String {
        format!("Not decoded (0x{})", hex::encode(value.as_bytes_ref()))
    }

}

#[derive(Serialize, Debug)]
pub struct Mapping {
    #[serde(flatten)]
    pub root: RootKeyEntry,
    #[serde(rename = "Map")]
    pub value: BTreeMap<Bytes, Bytes>,
    #[serde(skip_serializing)]
    param_value_type_id: u32,
    #[serde(skip_serializing)]
    param_key_type_id: u32,
}

impl DecodePrettyString for Mapping {
    fn decode_pretty_string(&self, decoder: &ContractMessageTranscoder) -> String {
        self.value
            .iter()
            .map(|(k, v)| {
                let key = decoder
                    .decode(self.param_key_type_id, &mut k.as_bytes_ref())
                    .map_or(Self::not_decoded_hex_bytes(k),  |e| e.to_string());
                let value = decoder
                    .decode(self.param_value_type_id, &mut v.as_bytes_ref())
                    .map_or(Self::not_decoded_hex_bytes(v),  |e| e.to_string());
                format!("Map {{ {} => {} }}\n", key, value)
            })
            .fold(String::new(), |s, e| {
                let s = s + e.as_str();
                s
            })
            .trim_end()
            .to_string()
    }
}

#[derive(Serialize, Debug)]
pub struct Lazy {
    #[serde(flatten)]
    pub root: RootKeyEntry,
    pub value: Bytes,
    #[serde(skip_serializing)]
    param_value_type_id: u32,
}

impl DecodePrettyString for Lazy {
    fn decode_pretty_string(&self, decoder: &ContractMessageTranscoder) -> String {
        if let Ok(value) =
            decoder.decode(self.param_value_type_id, &mut self.value.as_bytes_ref())
        {
            return value.to_string()
        }
        Self::not_decoded_hex_bytes(&self.value)
    }
}

#[derive(Serialize, Debug)]
pub struct StorageVec {
    #[serde(flatten)]
    pub root: RootKeyEntry,
    #[serde(rename = "Vec")]
    pub value: Vec<(Option<Bytes>, Bytes)>,
    #[serde(skip_serializing)]
    param_value_type_id: u32,
}

impl DecodePrettyString for StorageVec {
    fn decode_pretty_string(&self, decoder: &ContractMessageTranscoder) -> String {
        self.value
            .iter()
            .filter_map(|(k,v)| {
                if let Some(mapping_key) = k {
                    let key = u32::decode(&mut mapping_key.as_bytes_ref())
                    .map_or(Self::not_decoded_hex_bytes(mapping_key),  |e| e.to_string());
                    let value = decoder
                        .decode(self.param_value_type_id, &mut v.as_bytes_ref())
                        .map_or(Self::not_decoded_hex_bytes(v),  |e| e.to_string());
                    return Some(format!("StorageVec {{ [{}] => {} }}\n", key, value))
                }
                None
            })
            .fold(String::new(), |s, e| {
                let s = s + e.as_str();
                s
            })
            .trim_end()
            .to_string()
    }
}

#[derive(Serialize, Debug)]
pub struct Value {
    #[serde(flatten)]
    pub root: RootKeyEntry,
    pub value: Bytes,
}

impl DecodePrettyString for Value {
    fn decode_pretty_string(&self, decoder: &ContractMessageTranscoder) -> String {
        if let Ok(value) =
            decoder.decode(self.root.type_id, &mut self.value.as_bytes_ref())
        {
            return value.to_string()
        }
        Self::not_decoded_hex_bytes(&self.value)
    }
}

#[derive(Serialize, Debug)]
pub enum ContractStorageCell {
    Mapping(Mapping),
    Lazy(Lazy),
    StorageVec(StorageVec),
    Value(Value),
}

impl ContractStorageCell {
    fn root(&self) -> &RootKeyEntry {
        match self {
            Self::Mapping(mapping) => &mapping.root,
            Self::Lazy(lazy) => &lazy.root,
            Self::StorageVec(storage_vec) => &storage_vec.root,
            Self::Value(value) => &value.root,
        }
    }
    pub fn path(&self) -> String {
        self.root().path.join("::")
    }

    pub fn parent(&self) -> String {
        self.root().path.last().cloned().unwrap_or_default()
    }

    pub fn root_key(&self) -> String {
        hex::encode(self.root().root_key.to_le_bytes())
    }

    pub fn decode_pretty_string(&self, decoder: &ContractMessageTranscoder) -> String {
        match self {
            Self::Mapping(mapping) => mapping.decode_pretty_string(decoder),
            Self::Lazy(lazy) => lazy.decode_pretty_string(decoder),
            Self::StorageVec(storage_vec) => storage_vec.decode_pretty_string(decoder),
            Self::Value(value) => value.decode_pretty_string(decoder),
        }
    }
}

/// Represents storage cells containing values and type information for the contract.
#[derive(Serialize)]
pub struct ContractStorageLayout {
    pub cells: Vec<ContractStorageCell>,
}

impl ContractStorageLayout {
    ///  Creates a representation of contract storage based on raw storage entries and metadata.
    pub fn new(data: ContractStorageData, metadata: &InkProject) -> Self {
        let layout = metadata.layout();
        let registry = metadata.registry();
        let mut path_stack = vec!["root".to_string()];
        let mut root_key_entries: Vec<RootKeyEntry> = Vec::new();
        Self::collect_root_key_entries(layout, &mut path_stack, &mut root_key_entries);

        let mut cells = data
            .0
            .into_iter()
            .map(|(key, value)| {
                let (root_key, mapping_key) = Self::key_parts(&key);
                (root_key, (mapping_key, value))
            })
            .into_group_map()
            .into_iter()
            .filter_map(|(root_key, mut data)| {
                let root_key_entry =
                    root_key_entries.iter().find(|e| e.root_key == root_key)?;
                let type_def = registry.resolve(root_key_entry.type_id)?;
                let root = RootKeyEntry {
                    path: root_key_entry.path.clone(),
                    type_id: root_key_entry.type_id,
                    root_key,
                };
                match type_def.path.to_string().as_str() {
                    "ink_storage::lazy::mapping::Mapping" => {
                        let mapping_key_type_id = Self::param_type_id(type_def, "K")?;
                        let value_type_id = Self::param_type_id(type_def, "V")?;
                        let value = data
                            .into_iter()
                            .filter_map(|(k, v)| k.map(|key| (key, v)))
                            .collect();
                        Some(ContractStorageCell::Mapping(Mapping {
                            root,
                            value,
                            param_value_type_id: value_type_id,
                            param_key_type_id: mapping_key_type_id,
                        }))
                    }
                    "ink_storage::lazy::vec::StorageVec" => {
                        data.sort_by(|a, b| a.0.cmp(&b.0));
                        let value_type_id = Self::param_type_id(type_def, "V")?;
                        Some(ContractStorageCell::StorageVec(StorageVec {
                            root,
                            value: data,
                            param_value_type_id: value_type_id,
                        }))
                    }
                    "ink_storage::lazy::Lazy" => {
                        let value_type_id = Self::param_type_id(type_def, "V")?;

                        Some(ContractStorageCell::Lazy(Lazy {
                            root,
                            value: data.first()?.1.clone(),
                            param_value_type_id: value_type_id,
                        }))
                    }
                    _ => {
                        Some(ContractStorageCell::Value(Value {
                            root,
                            value: data.first()?.1.clone(),
                        }))
                    }
                }
            })
            .collect::<Vec<_>>();

        cells.sort_by_key(|k| k.path());

        Self { cells }
    }

    fn collect_root_key_entries(
        layout: &Layout<PortableForm>,
        path: &mut Vec<String>,
        entries: &mut Vec<RootKeyEntry>,
    ) {
        match layout {
            Layout::Root(root) => {
                entries.push(RootKeyEntry {
                    root_key: *root.root_key().key(),
                    path: path.clone(),
                    type_id: root.ty().id,
                });
                Self::collect_root_key_entries(root.layout(), path, entries);
            }
            Layout::Struct(struct_layout) => {
                Self::struct_entries(struct_layout, path, entries)
            }
            Layout::Enum(enum_layout) => {
                path.push(enum_layout.name().to_string());
                for (variant, struct_layout) in enum_layout.variants() {
                    path.push(variant.value().to_string());
                    Self::struct_entries(struct_layout, path, entries);
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
        entries: &mut Vec<RootKeyEntry>,
    ) {
        let struct_label = struct_layout.name().to_string();
        path.push(struct_label);
        for field in struct_layout.fields() {
            path.push(field.name().to_string());
            Self::collect_root_key_entries(field.layout(), path, entries);
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

    fn param_type_id(type_def: &Type<PortableForm>, param_name: &str) -> Option<u32> {
        Some(
            type_def
                .type_params
                .iter()
                .find(|&e| e.name == param_name)?
                .ty?
                .id,
        )
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

    /// Fetch the keys of the contract storage.
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

    /// Fetch the storage values for the given keys.
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
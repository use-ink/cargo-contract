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
    fetch_contract_info,
    url_to_string,
    ContractInfo,
    TrieId,
};
use anyhow::{
    anyhow,
    Result,
};
use contract_transcode::{
    ContractMessageTranscoder,
    Value,
};
use ink_env::Environment;
use ink_metadata::layout::{
    Layout,
    StructLayout,
};
use itertools::Itertools;
use scale::{
    Decode,
    Encode,
};
use scale_info::{
    form::PortableForm,
    Type,
};
use serde::{
    Serialize,
    Serializer,
};
use sp_core::{
    hexdisplay::AsBytesRef,
    storage::ChildInfo,
};
use std::{
    collections::BTreeMap,
    fmt::{
        self,
        Display,
        Formatter,
    },
    marker::PhantomData,
};
use subxt::{
    backend::{
        legacy::{
            rpc_methods::Bytes,
            LegacyRpcMethods,
        },
        rpc::RpcClient,
    },
    config::HashFor,
    ext::{
        scale_decode::IntoVisitor,
        subxt_rpcs::client::rpc_params,
    },
    utils::H160,
    Config,
    OnlineClient,
};

pub struct ContractStorage<C: Config, E: Environment> {
    rpc: ContractStorageRpc<C>,
    _phantom: PhantomData<fn() -> E>,
}

impl<C: Config, E: Environment> ContractStorage<C, E>
where
    C::AccountId: AsRef<[u8]> + Display + IntoVisitor,
    HashFor<C>: IntoVisitor,
    E::Balance: IntoVisitor + Serialize,
{
    pub fn new(rpc: ContractStorageRpc<C>) -> Self {
        Self {
            rpc,
            _phantom: Default::default(),
        }
    }

    /// Fetch the storage version of the pallet contracts.
    ///
    /// This is the result of a state query to the function `contracts::palletVersion())`.
    pub async fn version(&self) -> Result<u16> {
        self.rpc
            .client
            .storage()
            .at_latest()
            .await?
            .storage_version("Revive")
            .await
            .map_err(|e| {
                anyhow!("The storage version for the contracts pallet could not be determined: {e}")
            })
    }

    /// Load the raw key/value storage for a given contract.
    pub async fn load_contract_storage_data(
        &self,
        contract_account: &H160,
    ) -> Result<ContractStorageData>
    where
        C::AccountId: Decode,
    {
        let contract_info = self.rpc.fetch_contract_info::<E>(contract_account).await?;
        let trie_id = contract_info.trie_id();

        let mut storage_keys = Vec::new();
        let mut storage_values = Vec::new();
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
            let keys_count = keys.len();
            let mut values = self.rpc.fetch_storage_entries(trie_id, &keys, None).await?;
            assert_eq!(
                keys_count,
                values.len(),
                "storage keys and values must be the same length"
            );
            storage_keys.append(&mut keys);
            storage_values.append(&mut values);

            if (keys_count as u32) < KEYS_COUNT {
                break
            }
        }

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
        contract_addr: &H160,
        decoder: &ContractMessageTranscoder,
    ) -> Result<ContractStorageLayout>
    where
        C::AccountId: Decode,
    {
        let data = self.load_contract_storage_data(contract_addr).await?;
        ContractStorageLayout::new(data, decoder)
    }
}

/// Represents the raw key/value storage for the contract.
#[derive(Serialize, Debug)]
pub struct ContractStorageData(BTreeMap<Bytes, Bytes>);

impl ContractStorageData {
    /// Create a representation of raw contract storage
    pub fn new(data: BTreeMap<Bytes, Bytes>) -> Self {
        Self(data)
    }
}

/// Represents the RootLayout storage entry for the contract.
#[derive(Serialize, Debug)]
pub struct RootKeyEntry {
    #[serde(serialize_with = "RootKeyEntry::key_as_hex")]
    pub root_key: u32,
    pub path: Vec<String>,
    pub type_id: u32,
}

impl RootKeyEntry {
    fn key_as_hex<S>(key: &u32, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(format!("0x{}", hex::encode(key.encode())).as_str())
    }
}

#[derive(Serialize, Debug)]
pub struct Mapping {
    #[serde(flatten)]
    root: RootKeyEntry,
    map: Vec<(Value, Value)>,
}

impl Mapping {
    // Create new `Mapping`.
    pub fn new(root: RootKeyEntry, value: Vec<(Value, Value)>) -> Mapping {
        Mapping { root, map: value }
    }

    /// Return the root key entry of the `Mapping`.
    pub fn root(&self) -> &RootKeyEntry {
        &self.root
    }

    /// Iterate all key-value pairs.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &(Value, Value)> {
        self.map.iter()
    }
}

impl Display for Mapping {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let len = self.map.len();
        for (i, e) in self.map.iter().enumerate() {
            write!(f, "Mapping {{ {} => {} }}", e.0, e.1)?;
            if i + 1 < len {
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Debug)]
pub struct Lazy {
    #[serde(flatten)]
    root: RootKeyEntry,
    value: Value,
}

impl Lazy {
    /// Create new `Lazy`
    pub fn new(root: RootKeyEntry, value: Value) -> Lazy {
        Lazy { root, value }
    }

    /// Return the root key entry of the `Lazy`.
    pub fn root(&self) -> &RootKeyEntry {
        &self.root
    }

    /// Return the Lazy value.
    pub fn value(&self) -> &Value {
        &self.value
    }
}

impl Display for Lazy {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Lazy {{ {} }}", self.value)
    }
}

#[derive(Serialize, Debug)]
pub struct StorageVec {
    #[serde(flatten)]
    root: RootKeyEntry,
    len: u32,
    vec: Vec<Value>,
}

impl StorageVec {
    /// Create new `StorageVec`.
    pub fn new(root: RootKeyEntry, len: u32, value: Vec<Value>) -> StorageVec {
        StorageVec {
            root,
            len,
            vec: value,
        }
    }

    /// Return the root key entry of the `StorageVec`.
    pub fn root(&self) -> &RootKeyEntry {
        &self.root
    }

    // Return the len of the `StorageVec`.
    pub fn len(&self) -> u32 {
        self.len
    }

    /// Return the iterator over the `StorageVec` values.
    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.vec.iter()
    }
}

impl Display for StorageVec {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for (i, v) in self.vec.iter().enumerate() {
            write!(f, "StorageVec [{}] {{ [{}] => {} }}", self.len, i, v)?;
            if i + 1 < self.len as usize {
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Debug)]
pub struct Packed {
    #[serde(flatten)]
    root: RootKeyEntry,
    value: Value,
}

impl Packed {
    /// Create new `Packed`.
    pub fn new(root: RootKeyEntry, value: Value) -> Packed {
        Packed { root, value }
    }

    /// Return the root key entry of the `Packed`.
    pub fn root(&self) -> &RootKeyEntry {
        &self.root
    }

    /// Return the Packed value.
    pub fn value(&self) -> &Value {
        &self.value
    }
}

impl Display for Packed {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

/// Represents the storage cell value.
#[derive(Serialize, Debug)]
pub enum ContractStorageCell {
    Mapping(Mapping),
    Lazy(Lazy),
    StorageVec(StorageVec),
    Packed(Packed),
}

impl ContractStorageCell {
    fn root(&self) -> &RootKeyEntry {
        match self {
            Self::Mapping(mapping) => mapping.root(),
            Self::Lazy(lazy) => lazy.root(),
            Self::StorageVec(storage_vec) => storage_vec.root(),
            Self::Packed(packed) => packed.root(),
        }
    }

    /// Return the `RootKeyEntry` path as a string.
    pub fn path(&self) -> String {
        self.root().path.join("::")
    }

    /// Return the parent.
    pub fn parent(&self) -> String {
        self.root().path.last().cloned().unwrap_or_default()
    }

    /// Return the root_key as a hex-encoded string.
    pub fn root_key(&self) -> String {
        hex::encode(self.root().root_key.encode())
    }
}

impl Display for ContractStorageCell {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mapping(mapping) => mapping.fmt(f),
            Self::Lazy(lazy) => lazy.fmt(f),
            Self::StorageVec(storage_vec) => storage_vec.fmt(f),
            Self::Packed(value) => value.fmt(f),
        }
    }
}

/// Represents storage cells containing values and type information for the contract.
#[derive(Serialize, Debug)]
pub struct ContractStorageLayout {
    cells: Vec<ContractStorageCell>,
}

impl ContractStorageLayout {
    /// Create a representation of contract storage based on raw storage entries and
    /// metadata.
    pub fn new(
        data: ContractStorageData,
        decoder: &ContractMessageTranscoder,
    ) -> Result<Self> {
        let layout = decoder.metadata().layout();
        let registry = decoder.metadata().registry();
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
            .map(|(root_key, mut data)| {
                let root_key_entry = root_key_entries
                    .iter()
                    .find(|e| e.root_key == root_key)
                    .ok_or(anyhow!(
                        "Root key {} not found for the RootLayout",
                        root_key
                    ))?;
                let type_def = registry.resolve(root_key_entry.type_id).ok_or(
                    anyhow!("Type {} not found in the registry", root_key_entry.type_id),
                )?;
                let root = RootKeyEntry {
                    path: root_key_entry.path.clone(),
                    type_id: root_key_entry.type_id,
                    root_key,
                };
                match type_def.path.to_string().as_str() {
                    "ink_storage::lazy::mapping::Mapping" => {
                        let key_type_id = Self::param_type_id(type_def, "K")
                            .ok_or(anyhow!("Param `K` not found in type registry"))?;
                        let value_type_id = Self::param_type_id(type_def, "V")
                            .ok_or(anyhow!("Param `V` not found in type registry"))?;
                        let value = Self::decode_to_mapping(
                            data,
                            key_type_id,
                            value_type_id,
                            decoder,
                        )?;
                        Ok(ContractStorageCell::Mapping(Mapping::new(root, value)))
                    }
                    "ink_storage::lazy::vec::StorageVec" => {
                        // Sort by the key to get the Vec in the right order.
                        data.sort_by(|a, b| a.0.cmp(&b.0));
                        // First item is the `StorageVec` len.
                        let raw_len = data
                            .first()
                            .ok_or(anyhow!("Length of the StorageVec not found"))?
                            .1
                            .clone();
                        let len = u32::decode(&mut raw_len.as_bytes_ref())?;
                        let value_type_id = Self::param_type_id(type_def, "V")
                            .ok_or(anyhow!("Param `V` not found in type registry"))?;
                        let value =
                            Self::decode_to_vec(&data[1..], value_type_id, decoder)?;
                        Ok(ContractStorageCell::StorageVec(StorageVec::new(
                            root, len, value,
                        )))
                    }
                    "ink_storage::lazy::Lazy" => {
                        let value_type_id = Self::param_type_id(type_def, "V")
                            .ok_or(anyhow!("Param `V` not found in type registry"))?;
                        let raw_value =
                            data.first().ok_or(anyhow!("Empty storage cell"))?.1.clone();
                        let value = decoder
                            .decode(value_type_id, &mut raw_value.as_bytes_ref())?;
                        Ok(ContractStorageCell::Lazy(Lazy::new(root, value)))
                    }
                    _ => {
                        let raw_value =
                            data.first().ok_or(anyhow!("Empty storage cell"))?.1.clone();
                        let value = decoder
                            .decode(root.type_id, &mut raw_value.as_bytes_ref())?;
                        Ok(ContractStorageCell::Packed(Packed::new(root, value)))
                    }
                }
            })
            .collect::<Result<Vec<_>>>()?;

        cells.sort_by_key(|k| k.path());

        Ok(Self { cells })
    }

    /// Return the iterator over the storage cells.
    pub fn iter(&self) -> impl Iterator<Item = &ContractStorageCell> {
        self.cells.iter()
    }

    fn decode_to_mapping(
        data: Vec<(Option<Bytes>, Bytes)>,
        key_type_id: u32,
        value_type_id: u32,
        decoder: &ContractMessageTranscoder,
    ) -> Result<Vec<(Value, Value)>> {
        data.into_iter()
            .map(|(k, v)| {
                let k = k.ok_or(anyhow!("The Mapping key is missing in the map"))?;
                let key = decoder.decode(key_type_id, &mut k.as_bytes_ref())?;
                let value = decoder.decode(value_type_id, &mut v.as_bytes_ref())?;
                Ok((key, value))
            })
            .collect()
    }

    fn decode_to_vec(
        data: &[(Option<Bytes>, Bytes)],
        value_type_id: u32,
        decoder: &ContractMessageTranscoder,
    ) -> Result<Vec<Value>> {
        data.iter()
            .map(|(_, v)| {
                let value = decoder.decode(value_type_id, &mut v.as_bytes_ref())?;
                Ok(value)
            })
            .collect()
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
            Layout::Hash(_) => {
                unimplemented!("Layout::Hash is not currently be constructed")
            }
            Layout::Array(_) | Layout::Leaf(_) => {}
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

    /// Get the type id of the parameter name from the type.
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
    HashFor<C>: IntoVisitor,
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
    pub async fn fetch_contract_info<E: Environment>(
        &self,
        contract: &H160,
    ) -> Result<ContractInfo<E::Balance>>
    where
        C::AccountId: Decode,
        E::Balance: IntoVisitor,
    {
        fetch_contract_info::<C, E>(contract, &self.rpc_methods, &self.client).await
    }

    /// Fetch the contract storage at the given key.
    ///
    /// For more information about how storage keys are calculated see: https://use.ink/datastructures/storage-in-metadata
    pub async fn fetch_contract_storage(
        &self,
        trie_id: &TrieId,
        key: &Bytes,
        block_hash: Option<HashFor<C>>,
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
        block_hash: Option<HashFor<C>>,
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
        block_hash: Option<HashFor<C>>,
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

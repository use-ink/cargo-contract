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

use super::get_best_block;
use anyhow::{
    Result,
    anyhow,
};
use contract_metadata::byte_str::serialize_as_byte_str;
use std::fmt::{
    Debug,
    Display,
    Formatter,
};

use ink_env::Environment;
use scale::Decode;
use std::option::Option;
use subxt::{
    Config,
    OnlineClient,
    backend::legacy::LegacyRpcMethods,
    config::HashFor,
    ext::{
        scale_decode::{
            DecodeAsType,
            IntoVisitor,
        },
        scale_value::Value,
    },
    storage::dynamic,
    utils::{
        H160,
        H256,
    },
};

/// Return the account data for an account ID.
pub async fn get_account_data<C: Config, E: Environment>(
    account: &C::AccountId,
    rpc: &LegacyRpcMethods<C>,
    client: &OnlineClient<C>,
) -> Result<AccountData<E::Balance>>
where
    C::AccountId: AsRef<[u8]>,
    E::Balance: IntoVisitor,
{
    let storage_query =
        subxt::dynamic::storage("System", "Account", vec![Value::from_bytes(account)]);
    let best_block = get_best_block(rpc).await?;

    let account = client
        .storage()
        .at(best_block)
        .fetch(&storage_query)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Failed to fetch account data"))?;

    let data = account.as_type::<AccountInfo<E::Balance>>()?.data;
    Ok(data)
}

/// Returns the `AccountId32` for a `H160`.
///
/// If a mapping for `addr` is not found on the node, a fallback account will be returned.
pub async fn resolve_h160<C: Config, E: Environment>(
    addr: &H160,
    rpc: &LegacyRpcMethods<C>,
    client: &OnlineClient<C>,
) -> Result<C::AccountId>
where
    C::AccountId: AsRef<[u8]> + Display + IntoVisitor + Decode,
    HashFor<C>: IntoVisitor,
    E::Balance: IntoVisitor,
{
    let best_block = get_best_block(rpc).await?;
    let contract_info_address =
        dynamic("Revive", "OriginalAccount", vec![Value::from_bytes(addr)]);
    let raw_value = client
        .storage()
        .at(best_block)
        .fetch(&contract_info_address)
        .await?;
    match raw_value {
        None => {
            // This typically happens when calling this function with a contract, for
            // which there is no `AccountId`.
            fn to_fallback_account_id(address: &H160) -> [u8; 32] {
                let mut account_id = [0xEE; 32];
                account_id[..20].copy_from_slice(address.as_bytes());
                account_id
            }
            let fallback = to_fallback_account_id(addr);
            tracing::debug!(
                "No address suffix was found in the node for H160 address {:?}, using fallback {:?}",
                addr,
                fallback
            );
            let account_id =
                <C as Config>::AccountId::decode(&mut &fallback[..]).unwrap();
            Ok(account_id)
        }
        Some(raw_value) => {
            let raw_account_id = raw_value.as_type::<[u8; 32]>()?;
            let account: C::AccountId = Decode::decode(&mut &raw_account_id[..])
                .map_err(|err| {
                    anyhow!("AccountId from `[u8; 32]` deserialization error: {err}")
                })?;
            Ok(account)
        }
    }
}

/// Fetch the code info from the storage using the provided client.
pub async fn fetch_code_info<C: Config, E: Environment>(
    code_hash: &H256,
    rpc: &LegacyRpcMethods<C>,
    client: &OnlineClient<C>,
) -> Result<CodeInfo<C::AccountId, E::Balance>>
where
    HashFor<C>: IntoVisitor,
    C::AccountId: AsRef<[u8]> + Display + IntoVisitor + Decode + Debug,
    E::Balance: IntoVisitor + Debug,
{
    let best_block = get_best_block(rpc).await?;

    let code_info_address =
        dynamic("Revive", "CodeInfoOf", vec![Value::from_bytes(code_hash)]);
    let code_info_value = client
        .storage()
        .at(best_block)
        .fetch(&code_info_address)
        .await?
        .ok_or_else(|| anyhow!("No code info was found for hash {code_hash:?}"))?;
    let code_info = code_info_value.as_type::<CodeInfo<C::AccountId, E::Balance>>()?;
    Ok(CodeInfo {
        owner: code_info.owner,
        deposit: code_info.deposit,
        refcount: code_info.refcount,
        code_len: code_info.code_len,
        code_type: code_info.code_type,
        behaviour_version: code_info.behaviour_version,
    })
}

/// Fetch the contract info from the storage using the provided client.
pub async fn fetch_contract_info<C: Config, E: Environment>(
    contract: &H160,
    rpc: &LegacyRpcMethods<C>,
    client: &OnlineClient<C>,
) -> Result<ContractInfo<E::Balance>>
where
    HashFor<C>: IntoVisitor,
    C::AccountId: AsRef<[u8]> + Display + IntoVisitor + Decode + Debug,
    E::Balance: IntoVisitor + Debug,
{
    let best_block = get_best_block(rpc).await?;

    let account_info_address =
        dynamic("Revive", "AccountInfoOf", vec![Value::from_bytes(contract)]);
    let account_info_value = client
        .storage()
        .at(best_block)
        .fetch(&account_info_address)
        .await?
        .ok_or_else(|| anyhow!("No contract was found for address {contract:?}"))?;
    let account_info = account_info_value.as_type::<PrAccountInfo<E::Balance>>()?;

    let contract_info = match account_info.account_type {
        PrAccountType::Contract(contract_info) => contract_info,
        PrAccountType::Eoa => panic!("Contract address is an EOA!"),
    };
    Ok(ContractInfo::<E::Balance> {
        trie_id: contract_info.trie_id.0.into(),
        code_hash: contract_info.code_hash,
        storage_bytes: contract_info.storage_bytes,
        storage_items: contract_info.storage_items,
        storage_byte_deposit: contract_info.storage_byte_deposit,
        storage_item_deposit: contract_info.storage_item_deposit,
        storage_base_deposit: contract_info.storage_base_deposit,
        immutable_data_len: contract_info.immutable_data_len,
    })
}

/// Copied from `pallet-revive`.
///
/// Represents the account information for a contract or an externally owned account
/// (EOA).
#[derive(DecodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
pub struct PrAccountInfo<Balance: Debug + DecodeAsType> {
    /// The type of the account.
    pub account_type: PrAccountType<Balance>,

    // The  amount that was transferred to this account that is less than the
    // NativeToEthRatio, and can be represented in the native currency
    #[allow(dead_code)]
    pub dust: u32,
}

/// Copied from `pallet-revive`.
///
/// The account type is used to distinguish between contracts and externally owned
/// accounts.
#[derive(DecodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
pub enum PrAccountType<Balance: Debug + DecodeAsType> {
    /// An account that is a contract.
    Contract(PrContractInfo<Balance>),

    /// An account that is an externally owned account (EOA).
    Eoa,
}

/// Copied from `pallet-revive`. Used for deserializing data fetched from a node.
///
/// Struct representing contract info, supporting deposit on either the main or secondary
/// account.
#[derive(DecodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct PrContractInfo<Balance: Debug + IntoVisitor> {
    trie_id: TrieId,
    code_hash: H256,
    storage_bytes: u32,
    storage_items: u32,
    storage_byte_deposit: Balance,
    storage_item_deposit: Balance,
    storage_base_deposit: Balance,
    immutable_data_len: u32,
}

/// This is `PrContractInfo` plus the field `code_info`.
/// We use this internally in `cargo-contract` to track the information.
///
/// Struct representing contract info, supporting deposit on either the main or secondary
/// account.
#[derive(DecodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct ContractInfo<Balance: Debug + IntoVisitor> {
    trie_id: TrieId,
    code_hash: H256,
    storage_bytes: u32,
    storage_items: u32,
    storage_byte_deposit: Balance,
    storage_item_deposit: Balance,
    storage_base_deposit: Balance,
    immutable_data_len: u32,
}

impl<Balance> ContractInfo<Balance>
where
    Balance: serde::Serialize + Copy + IntoVisitor + Debug,
{
    /// Convert and return contract info in JSON format.
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Return the trie_id of the contract.
    pub fn trie_id(&self) -> &TrieId {
        &self.trie_id
    }

    /// Return the code_hash of the contract.
    pub fn code_hash(&self) -> &H256 {
        &self.code_hash
    }

    /// Return the number of storage items of the contract.
    pub fn storage_bytes(&self) -> u32 {
        self.storage_bytes
    }

    /// Return the number of storage items of the contract.
    pub fn storage_items(&self) -> u32 {
        self.storage_items
    }

    /// Return the storage item deposit of the contract.
    pub fn storage_item_deposit(&self) -> Balance {
        self.storage_item_deposit
    }

    /// Return the storage item deposit of the contract.
    pub fn storage_byte_deposit(&self) -> Balance {
        self.storage_byte_deposit
    }

    /// Return the storage item deposit of the contract.
    pub fn storage_base_deposit(&self) -> Balance {
        self.storage_base_deposit
    }

    /// Return the storage item deposit of the contract.
    pub fn immutable_data_len(&self) -> u32 {
        self.immutable_data_len
    }
}

/// Copied from `pallet-revive`.
///
/// Contract code related data, such as:
///
/// - owner of the contract, i.e. account uploaded its code,
/// - storage deposit amount,
/// - reference count,
///
/// It is stored in a separate storage entry to avoid loading the code when not necessary.
#[derive(DecodeAsType, Eq, PartialEq, Clone, Debug, serde::Serialize)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
pub struct CodeInfo<
    AccountId: Debug + DecodeAsType + IntoVisitor,
    Balance: Debug + DecodeAsType + IntoVisitor,
> {
    /// The account that has uploaded the contract code and hence is allowed to remove
    /// it.
    pub owner: AccountId,
    /// The amount of balance that was deposited by the owner in order to store it
    /// on-chain.
    #[codec(compact)]
    pub deposit: Balance,
    /// The number of instantiated contracts that use this as their code.
    #[codec(compact)]
    pub refcount: u64,
    /// Length of the code in bytes.
    pub code_len: u32,
    /// Bytecode type
    pub code_type: BytecodeType,
    /// The behaviour version that this contract operates under.
    ///
    /// Whenever any observeable change (with the exception of weights) are made we need
    /// to make sure that already deployed contracts will not be affected. We do this by
    /// exposing the old behaviour depending on the set behaviour version of the
    /// contract.
    ///
    /// As of right now this is a reserved field that is always set to 0.
    pub behaviour_version: u32,
}

/// Copied from `pallet-revive`.
#[derive(PartialEq, Eq, Debug, Copy, Clone, DecodeAsType, serde::Serialize)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
pub enum BytecodeType {
    /// The code is a PVM bytecode.
    Pvm,
    /// The code is an EVM bytecode.
    Evm,
}

/// A contract's child trie id.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, DecodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
pub struct TrieId(#[serde(serialize_with = "serialize_as_byte_str")] Vec<u8>);

impl TrieId {
    /// Encode the trie id as hex string.
    pub fn to_hex(&self) -> String {
        format!("0x{}", hex::encode(&self.0))
    }
}

impl From<Vec<u8>> for TrieId {
    fn from(raw: Vec<u8>) -> Self {
        Self(raw)
    }
}

impl AsRef<[u8]> for TrieId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl Display for TrieId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Fetch the contract binary from the storage using the provided client and code hash.
pub async fn fetch_contract_binary<C: Config>(
    client: &OnlineClient<C>,
    rpc: &LegacyRpcMethods<C>,
    hash: &H256,
) -> Result<Vec<u8>> {
    let best_block = get_best_block(rpc).await?;

    let pristine_code_address =
        dynamic("Revive", "PristineCode", vec![Value::from_bytes(hash)]);
    let pristine_code = client
        .storage()
        .at(best_block)
        .fetch(&pristine_code_address)
        .await?
        .ok_or_else(|| anyhow!("No contract binary was found for code hash {hash}"))?;
    pristine_code
        .as_type::<Vec<u8>>()
        .map_err(|e| anyhow!("Contract binary could not be parsed: {e}"))
}

/// Parse a contract account address from a storage key. Returns error if a key is
/// malformed.
fn parse_contract_address(
    storage_contract_account_key: &[u8],
    storage_contract_root_key_len: usize,
) -> Result<H160> {
    let mut account = storage_contract_account_key
        .get(storage_contract_root_key_len..)
        .ok_or(anyhow!("Unexpected storage key size"))?;
    Decode::decode(&mut account)
        .map_err(|err| anyhow!("H160 deserialization error: {err}"))
}

/// Fetch all contract addresses from the storage using the provided client.
pub async fn fetch_all_contracts<C: Config>(
    client: &OnlineClient<C>,
    rpc: &LegacyRpcMethods<C>,
) -> Result<Vec<H160>> {
    let best_block = get_best_block(rpc).await?;
    let root_key = subxt::dynamic::storage("Revive", "AccountInfoOf", ()).to_root_bytes();
    let mut keys = client
        .storage()
        .at(best_block)
        .fetch_raw_keys(root_key.clone())
        .await?;

    let mut contract_accounts = Vec::new();
    while let Some(result) = keys.next().await {
        let key = result?;
        let contract_account = parse_contract_address(&key, root_key.len())?;
        contract_accounts.push(contract_account);
    }

    Ok(contract_accounts)
}

/// A struct used in the storage reads to access account info.
#[derive(DecodeAsType, Debug)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
struct AccountInfo<Balance> {
    data: AccountData<Balance>,
}

/// A struct used in the storage reads to access account data.
#[derive(Clone, Debug, DecodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
pub struct AccountData<Balance> {
    pub free: Balance,
    pub reserved: Balance,
    pub frozen: Balance,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ink_env::DefaultEnvironment;
    use scale::Encode;
    use scale_info::{
        IntoPortable,
        Path,
    };
    use subxt::{
        dynamic::DecodedValueThunk,
        metadata::{
            DecodeWithMetadata,
            types::Metadata,
        },
    };

    // Find the type index in the metadata.
    fn get_metadata_type_index(
        ident: &'static str,
        module_path: &'static str,
        metadata: &Metadata,
    ) -> Result<usize> {
        let contract_info_path =
            Path::new(ident, module_path).into_portable(&mut Default::default());

        metadata
            .types()
            .types
            .iter()
            .enumerate()
            .find_map(|(i, t)| {
                if t.ty.path == contract_info_path {
                    Some(i)
                } else {
                    None
                }
            })
            .ok_or(anyhow!("Type not found"))
    }

    #[test]
    fn contract_info_v16_decode_works() {
        // This version of metadata does not include the deposit_account field in
        // ContractInfo
        #[subxt::subxt(
            runtime_metadata_path = "src/test_runtime_api/metadata_v16.scale",
            derive_for_type(
                path = "pallet_revive::storage::ContractInfo",
                derive = "scale::Encode",
                recursive
            )
        )]
        mod api_v16 {}

        use api_v16::runtime_types::{
            bounded_collections::bounded_vec::BoundedVec,
            pallet_revive::storage::ContractInfo as ContractInfoV16,
        };

        let metadata_bytes = std::fs::read("src/test_runtime_api/metadata_v16.scale")
            .expect("the metadata must be present");
        let metadata =
            Metadata::decode(&mut &*metadata_bytes).expect("the metadata must decode");
        let contract_info_type_id =
            get_metadata_type_index("ContractInfo", "pallet_revive::storage", &metadata)
                .expect("the contract info type must be present in the metadata");

        let contract_info_v16 = ContractInfoV16 {
            trie_id: BoundedVec(vec![]),
            code_hash: Default::default(),
            storage_bytes: 1,
            storage_items: 1,
            storage_byte_deposit: 1,
            storage_item_deposit: 1,
            storage_base_deposit: 1,
            immutable_data_len: 1,
        };

        let contract_info_thunk = DecodedValueThunk::decode_with_metadata(
            &mut contract_info_v16.encode().as_slice(),
            contract_info_type_id as u32,
            &metadata.into(),
        )
        .expect("the contract info must be decoded");
        let contract_info = contract_info_thunk
            .as_type::<PrContractInfo<<DefaultEnvironment as Environment>::Balance>>()
            .expect("failed");

        assert_eq!(
            contract_info,
            PrContractInfo {
                trie_id: contract_info_v16.trie_id.0.into(),
                code_hash: contract_info_v16.code_hash,
                storage_bytes: contract_info_v16.storage_bytes,
                storage_items: contract_info_v16.storage_items,
                storage_byte_deposit: contract_info_v16.storage_byte_deposit,
                storage_item_deposit: contract_info_v16.storage_item_deposit,
                storage_base_deposit: contract_info_v16.storage_base_deposit,
                immutable_data_len: contract_info_v16.immutable_data_len,
                // todo
                // storage_total_deposit: account_data.reserved,
            }
        );
    }
}

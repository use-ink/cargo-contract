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
    anyhow,
    Result,
};
use contract_metadata::byte_str::serialize_as_byte_str;
use std::fmt::{
    Display,
    Formatter,
};

use ink_env::Environment;
use pallet_revive::evm::H256;
use scale::Decode;
use std::option::Option;
use subxt::{
    backend::legacy::LegacyRpcMethods,
    config::HashFor,
    dynamic::DecodedValueThunk,
    ext::{
        scale_decode::{
            DecodeAsType,
            IntoVisitor,
        },
        scale_value::Value,
    },
    storage::dynamic,
    utils::H160,
    Config,
    OnlineClient,
};
//use contract_transcode::env_types::AccountId;

/// Return the account data for an account ID.
async fn get_account_balance<C: Config, E: Environment>(
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

/// Map a Ethereum address to its original `AccountId32`.
///
/// Stores the last 12 byte for addresses that were originally an `AccountId32` instead
/// of an `H160`. Register your `AccountId32` using [`Pallet::map_account`] in order to
/// use it with this pallet.
/// #[pallet::storage]
/// pub(crate) type AddressSuffix<T: Config> = StorageMap<_, Identity, H160, [u8; 12]>
/// Fetch the contract info from the storage using the provided client.
pub async fn fetch_mapped_account<C: Config, E: Environment>(
    contract: &H160,
    _rpc: &LegacyRpcMethods<C>,
    _client: &OnlineClient<C>,
) -> Result<C::AccountId>
where
    C::AccountId: AsRef<[u8]> + Display + IntoVisitor + Decode,
    HashFor<C>: IntoVisitor,
    E::Balance: IntoVisitor,
{
    let mut raw_account_id = [0xEE; 32];
    raw_account_id[..20].copy_from_slice(&contract.0[..20]);
    Decode::decode(&mut &raw_account_id[..])
        .map_err(|err| anyhow!("AccountId deserialization error: {}", err))
}

/// Returns the `AccountId32` for a `H160`.
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
        dynamic("Revive", "AddressSuffix", vec![Value::from_bytes(addr)]);
    let raw_value = client
        .storage()
        .at(best_block)
        .fetch(&contract_info_address)
        .await?
        .ok_or_else(|| {
            anyhow!("No address suffix was found for H160 address {:?}", addr)
        })?;

    let suffix = raw_value.as_type::<[u8; 12]>()?;

    let mut raw_account_id = [0u8; 32];
    raw_account_id[..20].copy_from_slice(&addr.0[..20]);
    raw_account_id[20..].copy_from_slice(&suffix[..12]);

    let account: C::AccountId = Decode::decode(&mut &raw_account_id[..])
        .map_err(|err| anyhow!("AccountId deserialization error: {}", err))?;
    Ok(account)
}

/// Fetch the contract info from the storage using the provided client.
pub async fn fetch_contract_info<C: Config, E: Environment>(
    contract: &H160,
    rpc: &LegacyRpcMethods<C>,
    client: &OnlineClient<C>,
) -> Result<ContractInfo<E::Balance>>
where
    HashFor<C>: IntoVisitor,
    C::AccountId: AsRef<[u8]> + Display + IntoVisitor + Decode,
    E::Balance: IntoVisitor,
{
    let best_block = get_best_block(rpc).await?;

    let contract_info_address = dynamic(
        "Revive",
        "ContractInfoOf",
        vec![Value::from_bytes(contract)],
    );
    let contract_info_value = client
        .storage()
        .at(best_block)
        .fetch(&contract_info_address)
        .await?
        .ok_or_else(|| anyhow!("No contract was found for address {:?}", contract))?;

    let contract_info_raw = ContractInfoRaw::<E>::new(*contract, contract_info_value)?;
    let addr = contract_info_raw.get_addr();

    let account = fetch_mapped_account::<C, E>(addr, rpc, client).await?;
    let deposit_account_data = get_account_balance::<C, E>(&account, rpc, client).await?;
    Ok(contract_info_raw.into_contract_info(deposit_account_data))
}

/// Struct representing contract info, supporting deposit on either the main or secondary
/// account.
struct ContractInfoRaw<E: Environment> {
    addr: H160,
    contract_info: ContractInfoOf<E::Balance>,
}

impl<E: Environment> ContractInfoRaw<E>
where
    E::Balance: IntoVisitor,
{
    /// Create a new instance of `ContractInfoRaw` based on the provided contract and
    /// contract info value.
    pub fn new(addr: H160, contract_info_value: DecodedValueThunk) -> Result<Self> {
        let contract_info =
            contract_info_value.as_type::<ContractInfoOf<E::Balance>>()?;
        Ok(Self {
            addr,
            contract_info,
        })
    }

    pub fn get_addr(&self) -> &H160 {
        &self.addr
    }

    /// Convert `ContractInfoRaw` to `ContractInfo`
    pub fn into_contract_info(
        self,
        deposit: AccountData<E::Balance>,
    ) -> ContractInfo<E::Balance> {
        ContractInfo {
            trie_id: self.contract_info.trie_id.0.into(),
            code_hash: self.contract_info.code_hash,
            storage_items: self.contract_info.storage_items,
            storage_items_deposit: self.contract_info.storage_item_deposit,
            storage_total_deposit: deposit.reserved,
        }
    }
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct ContractInfo<Balance> {
    trie_id: TrieId,
    code_hash: H256,
    storage_items: u32,
    storage_items_deposit: Balance,
    storage_total_deposit: Balance,
}

impl<Balance> ContractInfo<Balance>
where
    Balance: serde::Serialize + Copy,
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
    pub fn storage_items(&self) -> u32 {
        self.storage_items
    }

    /// Return the storage item deposit of the contract.
    pub fn storage_items_deposit(&self) -> Balance {
        self.storage_items_deposit
    }

    /// Return the storage item deposit of the contract.
    pub fn storage_total_deposit(&self) -> Balance {
        self.storage_total_deposit
    }
}

/// A contract's child trie id.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
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
        .ok_or_else(|| anyhow!("No contract binary was found for code hash {}", hash))?;
    let pristine_code = pristine_code
        .as_type::<BoundedVec<u8>>()
        .map_err(|e| anyhow!("Contract binary could not be parsed: {e}"));
    pristine_code.map(|v| v.0)
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
        .map_err(|err| anyhow!("H160 deserialization error: {}", err))
}

/// Fetch all contract addresses from the storage using the provided client.
pub async fn fetch_all_contracts<C: Config>(
    client: &OnlineClient<C>,
    rpc: &LegacyRpcMethods<C>,
) -> Result<Vec<H160>> {
    let best_block = get_best_block(rpc).await?;
    let root_key =
        subxt::dynamic::storage("Revive", "ContractInfoOf", ()).to_root_bytes();
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
struct AccountData<Balance> {
    reserved: Balance,
}

/// A struct representing `Vec`` used in the storage reads.
#[derive(Debug, DecodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
struct BoundedVec<T>(pub ::std::vec::Vec<T>);

/// A struct used in the storage reads to access contract info.
#[derive(Debug, DecodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
struct ContractInfoOf<Balance> {
    trie_id: BoundedVec<u8>,
    code_hash: H256,
    storage_items: u32,
    storage_item_deposit: Balance,
}

#[cfg(test)]
mod tests {
    /*
    use super::*;
    use ink_env::DefaultEnvironment;
    use scale::Encode;
    use scale_info::{
        IntoPortable,
        Path,
    };
    use subxt::{
        metadata::{
            types::Metadata,
            DecodeWithMetadata,
        },
        utils::AccountId32,
        PolkadotConfig as DefaultConfig,
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
    fn contract_info_v15_decode_works() {
        // This version of metadata does not include the deposit_account field in
        // ContractInfo
        #[subxt::subxt(runtime_metadata_path = "src/test_runtime_api/metadata_v15.scale")]
        mod api_v15 {}

        use api_v15::runtime_types::{
            bounded_collections::{
                bounded_btree_map::BoundedBTreeMap,
                bounded_vec::BoundedVec,
            },
            pallet_revive::storage::ContractInfo as ContractInfoV15,
        };

        let metadata_bytes = std::fs::read("src/test_runtime_api/metadata_v15.scale")
            .expect("the metadata must be present");
        let metadata =
            Metadata::decode(&mut &*metadata_bytes).expect("the metadata must decode");
        let contract_info_type_id = get_metadata_type_index(
            "ContractInfo",
            "pallet_revive::storage",
            &metadata,
        )
        .expect("the contract info type must be present in the metadata");

        let contract_info_v15 = ContractInfoV15 {
            trie_id: BoundedVec(vec![]),
            code_hash: Default::default(),
            storage_bytes: 1,
            storage_items: 1,
            storage_byte_deposit: 1,
            storage_item_deposit: 1,
            storage_base_deposit: 1,
            delegate_dependencies: BoundedBTreeMap(vec![]),
        };

        let contract_info_thunk = DecodedValueThunk::decode_with_metadata(
            &mut &*contract_info_v15.encode(),
            contract_info_type_id as u32,
            &metadata.into(),
        )
        .expect("the contract info must be decoded");

        let contract = AccountId32([0u8; 32]);
        let contract_info_raw =
            ContractInfoRaw::<DefaultConfig, DefaultEnvironment>::new(
                contract,
                contract_info_thunk,
            )
            .expect("the conatract info raw must be created");
        let account_data = AccountData {
            free: 1,
            reserved: 10,
        };

        let contract_info = contract_info_raw.into_contract_info(account_data.clone());
        assert_eq!(
            contract_info,
            ContractInfo {
                trie_id: contract_info_v15.trie_id.0.into(),
                code_hash: contract_info_v15.code_hash,
                storage_items: contract_info_v15.storage_items,
                storage_items_deposit: contract_info_v15.storage_item_deposit,
                storage_total_deposit: account_data.reserved,
            }
        );
    }
    */
}

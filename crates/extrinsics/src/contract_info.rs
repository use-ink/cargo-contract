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
use scale::Decode;
use std::option::Option;
use subxt::{
    backend::legacy::LegacyRpcMethods,
    dynamic::DecodedValueThunk,
    ext::{
        scale_decode::{
            DecodeAsType,
            IntoVisitor,
        },
        scale_value::Value,
    },
    storage::dynamic,
    Config,
    OnlineClient,
};

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

/// Fetch the contract info from the storage using the provided client.
pub async fn fetch_contract_info<C: Config, E: Environment>(
    contract: &C::AccountId,
    rpc: &LegacyRpcMethods<C>,
    client: &OnlineClient<C>,
) -> Result<ContractInfo<C::Hash, E::Balance>>
where
    C::AccountId: AsRef<[u8]> + Display + IntoVisitor,
    C::Hash: IntoVisitor,
    E::Balance: IntoVisitor,
{
    let best_block = get_best_block(rpc).await?;

    let contract_info_address = dynamic(
        "Contracts",
        "ContractInfoOf",
        vec![Value::from_bytes(contract)],
    );
    let contract_info_value = client
        .storage()
        .at(best_block)
        .fetch(&contract_info_address)
        .await?
        .ok_or_else(|| {
            anyhow!(
                "No contract information was found for account id {}",
                contract
            )
        })?;

    let contract_info_raw =
        ContractInfoRaw::<C, E>::new(contract.clone(), contract_info_value)?;
    let deposit_account = contract_info_raw.get_deposit_account();

    let deposit_account_data =
        get_account_balance::<C, E>(deposit_account, rpc, client).await?;
    Ok(contract_info_raw.into_contract_info(deposit_account_data))
}

/// Struct representing contract info, supporting deposit on either the main or secondary
/// account.
struct ContractInfoRaw<C: Config, E: Environment> {
    deposit_account: C::AccountId,
    contract_info: ContractInfoOf<C::Hash, E::Balance>,
    deposit_on_main_account: bool,
}

impl<C: Config, E: Environment> ContractInfoRaw<C, E>
where
    C::AccountId: IntoVisitor,
    C::Hash: IntoVisitor,
    E::Balance: IntoVisitor,
{
    /// Create a new instance of `ContractInfoRaw` based on the provided contract and
    /// contract info value. Determines whether it's a main or secondary account deposit.
    pub fn new(
        contract_account: C::AccountId,
        contract_info_value: DecodedValueThunk,
    ) -> Result<Self> {
        let contract_info =
            contract_info_value.as_type::<ContractInfoOf<C::Hash, E::Balance>>()?;
        // Pallet-contracts [>=10, <15] store the contract's deposit as a free balance
        // in a secondary account (deposit account). Other versions store it as
        // reserved balance on the main contract's account. If the
        // `deposit_account` field is present in a contract info structure,
        // the contract's deposit is in this account.
        match Self::get_deposit_account_id(&contract_info_value) {
            Ok(deposit_account) => {
                Ok(Self {
                    deposit_account,
                    contract_info,
                    deposit_on_main_account: false,
                })
            }
            Err(_) => {
                Ok(Self {
                    deposit_account: contract_account,
                    contract_info,
                    deposit_on_main_account: true,
                })
            }
        }
    }

    pub fn get_deposit_account(&self) -> &C::AccountId {
        &self.deposit_account
    }

    /// Convert `ContractInfoRaw` to `ContractInfo`
    pub fn into_contract_info(
        self,
        deposit: AccountData<E::Balance>,
    ) -> ContractInfo<C::Hash, E::Balance> {
        let total_deposit = if self.deposit_on_main_account {
            deposit.reserved
        } else {
            deposit.free
        };

        ContractInfo {
            trie_id: self.contract_info.trie_id.0.into(),
            code_hash: self.contract_info.code_hash,
            storage_items: self.contract_info.storage_items,
            storage_items_deposit: self.contract_info.storage_item_deposit,
            storage_total_deposit: total_deposit,
        }
    }

    /// Decode the deposit account from the contract info
    fn get_deposit_account_id(contract_info: &DecodedValueThunk) -> Result<C::AccountId> {
        let account = contract_info.as_type::<DepositAccount<C::AccountId>>()?;
        Ok(account.deposit_account)
    }
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct ContractInfo<Hash, Balance> {
    trie_id: TrieId,
    code_hash: Hash,
    storage_items: u32,
    storage_items_deposit: Balance,
    storage_total_deposit: Balance,
}

impl<Hash, Balance> ContractInfo<Hash, Balance>
where
    Hash: serde::Serialize,
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
    pub fn code_hash(&self) -> &Hash {
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

/// Fetch the contract wasm code from the storage using the provided client and code hash.
pub async fn fetch_wasm_code<C: Config>(
    client: &OnlineClient<C>,
    rpc: &LegacyRpcMethods<C>,
    hash: &C::Hash,
) -> Result<Vec<u8>>
where
    C::Hash: AsRef<[u8]> + Display + IntoVisitor,
{
    let best_block = get_best_block(rpc).await?;

    let pristine_code_address =
        dynamic("Contracts", "PristineCode", vec![Value::from_bytes(hash)]);
    let pristine_code = client
        .storage()
        .at(best_block)
        .fetch(&pristine_code_address)
        .await?
        .ok_or_else(|| anyhow!("No WASM code was found for code hash {}", hash))?;
    let pristine_code = pristine_code
        .as_type::<BoundedVec<u8>>()
        .map_err(|e| anyhow!("Contract wasm code could not be parsed: {e}"));
    pristine_code.map(|v| v.0)
}

/// Parse a contract account address from a storage key. Returns error if a key is
/// malformated.
fn parse_contract_account_address<C: Config>(
    storage_contract_account_key: &[u8],
    storage_contract_root_key_len: usize,
) -> Result<C::AccountId>
where
    C::AccountId: Decode,
{
    // storage_contract_account_key is a concatenation of contract_info_of root key and
    // Twox64Concat(AccountId)
    let mut account = storage_contract_account_key
        .get(storage_contract_root_key_len + 8..)
        .ok_or(anyhow!("Unexpected storage key size"))?;
    Decode::decode(&mut account)
        .map_err(|err| anyhow!("AccountId deserialization error: {}", err))
}

/// Fetch all contract addresses from the storage using the provided client.
pub async fn fetch_all_contracts<C: Config>(
    client: &OnlineClient<C>,
    rpc: &LegacyRpcMethods<C>,
) -> Result<Vec<C::AccountId>>
where
    C::AccountId: Decode,
{
    let best_block = get_best_block(rpc).await?;
    let root_key =
        subxt::dynamic::storage("Contracts", "ContractInfoOf", ()).to_root_bytes();
    let mut keys = client
        .storage()
        .at(best_block)
        .fetch_raw_keys(root_key.clone())
        .await?;

    let mut contract_accounts = Vec::new();
    while let Some(result) = keys.next().await {
        let key = result?;
        let contract_account = parse_contract_account_address::<C>(&key, root_key.len())?;
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
    free: Balance,
    reserved: Balance,
}

/// A struct representing `Vec`` used in the storage reads.
#[derive(Debug, DecodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
struct BoundedVec<T>(pub ::std::vec::Vec<T>);

/// A struct used in the storage reads to access contract info.
#[derive(Debug, DecodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
struct ContractInfoOf<Hash, Balance> {
    trie_id: BoundedVec<u8>,
    code_hash: Hash,
    storage_items: u32,
    storage_item_deposit: Balance,
}

/// A struct used in storage reads to access the deposit account from contract info.
#[derive(Debug, DecodeAsType)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
struct DepositAccount<AccountId> {
    deposit_account: AccountId,
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
    fn contract_info_v11_decode_works() {
        // This version of metadata includes the deposit_account field in ContractInfo
        #[subxt::subxt(runtime_metadata_path = "src/test_runtime_api/metadata_v11.scale")]
        mod api_v11 {}

        use api_v11::runtime_types::{
            bounded_collections::bounded_vec::BoundedVec,
            pallet_contracts::storage::{
                ContractInfo as ContractInfoV11,
                DepositAccount,
            },
        };

        let metadata_bytes = std::fs::read("src/test_runtime_api/metadata_v11.scale")
            .expect("the metadata must be present");
        let metadata =
            Metadata::decode(&mut &*metadata_bytes).expect("the metadata must decode");
        let contract_info_type_id = get_metadata_type_index(
            "ContractInfo",
            "pallet_contracts::storage",
            &metadata,
        )
        .expect("the contract info type must be present in the metadata");

        let contract_info_v11 = ContractInfoV11 {
            trie_id: BoundedVec(vec![]),
            deposit_account: DepositAccount(AccountId32([7u8; 32])),
            code_hash: Default::default(),
            storage_bytes: 1,
            storage_items: 1,
            storage_byte_deposit: 1,
            storage_item_deposit: 1,
            storage_base_deposit: 1,
        };

        let contract_info_thunk = DecodedValueThunk::decode_with_metadata(
            &mut &*contract_info_v11.encode(),
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
                trie_id: contract_info_v11.trie_id.0.into(),
                code_hash: contract_info_v11.code_hash,
                storage_items: contract_info_v11.storage_items,
                storage_items_deposit: contract_info_v11.storage_item_deposit,
                storage_total_deposit: account_data.free,
            }
        );
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
            pallet_contracts::storage::ContractInfo as ContractInfoV15,
        };

        let metadata_bytes = std::fs::read("src/test_runtime_api/metadata_v15.scale")
            .expect("the metadata must be present");
        let metadata =
            Metadata::decode(&mut &*metadata_bytes).expect("the metadata must decode");
        let contract_info_type_id = get_metadata_type_index(
            "ContractInfo",
            "pallet_contracts::storage",
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
}

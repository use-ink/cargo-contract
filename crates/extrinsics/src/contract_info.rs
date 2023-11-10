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
    Balance,
    Client,
    CodeHash,
    DefaultConfig,
};

use scale::Decode;
use std::option::Option;
use subxt::{
    backend::legacy::LegacyRpcMethods,
    dynamic::DecodedValueThunk,
    ext::{
        scale_decode::DecodeAsType,
        scale_value::Value,
    },
    storage::dynamic,
    utils::AccountId32,
};

#[derive(DecodeAsType, Debug)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
struct AccountData {
    pub free: Balance,
    pub reserved: Balance,
}

/// Return the account data for an account ID.
async fn get_account_balance(
    account: &AccountId32,
    rpc: &LegacyRpcMethods<DefaultConfig>,
    client: &Client,
) -> Result<AccountData> {
    let storage_query =
        subxt::dynamic::storage("System", "Account", vec![Value::from_bytes(account)]);
    let best_block = get_best_block(rpc).await?;

    let account = client
        .storage()
        .at(best_block)
        .fetch(&storage_query)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Failed to fetch account data"))?;

    #[derive(DecodeAsType, Debug)]
    #[decode_as_type(crate_path = "subxt::ext::scale_decode")]
    struct AccountInfo {
        data: AccountData,
    }

    let data = account.as_type::<AccountInfo>()?.data;
    Ok(data)
}

/// Decode the deposit account from the contract info
fn get_deposit_account_id(contract_info: &DecodedValueThunk) -> Result<AccountId32> {
    #[derive(DecodeAsType)]
    #[decode_as_type(crate_path = "subxt::ext::scale_decode")]
    struct DepositAccount {
        deposit_account: AccountId32,
    }

    let account = contract_info.as_type::<DepositAccount>()?;
    Ok(account.deposit_account)
}

/// Fetch the contract info from the storage using the provided client.
pub async fn fetch_contract_info(
    contract: &AccountId32,
    rpc: &LegacyRpcMethods<DefaultConfig>,
    client: &Client,
) -> Result<ContractInfo> {
    let best_block = get_best_block(rpc).await?;

    let contract_info_address = dynamic(
        "Contracts",
        "ContractInfoOf",
        vec![Value::from_bytes(contract)],
    );
    let contract_info = client
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
    #[derive(DecodeAsType, Debug)]
    #[decode_as_type(crate_path = "subxt::ext::scale_decode")]
    pub struct BoundedVec<T>(pub ::std::vec::Vec<T>);
    #[derive(DecodeAsType, Debug)]
    #[decode_as_type(crate_path = "subxt::ext::scale_decode")]
    struct ContractInfoOf {
        trie_id: BoundedVec<u8>,
        code_hash: CodeHash,
        storage_items: u32,
        storage_item_deposit: Balance,
    }

    // Pallet-contracts [>=10, <15] store the contract's deposit as a free balance
    // in a secondary account (deposit account). Other versions store it as
    // reserved balance on the main contract's account. If the
    // `deposit_account` field is present in a contract info structure,
    // the contract's deposit is in this account.
    let total_balance: Balance = match get_deposit_account_id(&contract_info) {
        Ok(deposit_account) => {
            get_account_balance(&deposit_account, rpc, client)
                .await?
                .free
        }
        Err(_) => get_account_balance(contract, rpc, client).await?.reserved,
    };

    let info = contract_info.as_type::<ContractInfoOf>()?;
    Ok(ContractInfo {
        trie_id: hex::encode(info.trie_id.0),
        code_hash: info.code_hash,
        storage_items: info.storage_items,
        storage_items_deposit: info.storage_item_deposit,
        storage_total_deposit: total_balance,
    })
}

#[derive(serde::Serialize)]
pub struct ContractInfo {
    trie_id: String,
    code_hash: CodeHash,
    storage_items: u32,
    storage_items_deposit: Balance,
    storage_total_deposit: Balance,
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
    pub fn storage_items_deposit(&self) -> Balance {
        self.storage_items_deposit
    }

    /// Return the storage item deposit of the contract.
    pub fn storage_total_deposit(&self) -> Balance {
        self.storage_total_deposit
    }
}

/// Fetch the contract wasm code from the storage using the provided client and code hash.
pub async fn fetch_wasm_code(
    client: &Client,
    rpc: &LegacyRpcMethods<DefaultConfig>,
    hash: &CodeHash,
) -> Result<Option<Vec<u8>>> {
    let pristine_code_address = api::storage().contracts().pristine_code(hash);
    let best_block = get_best_block(rpc).await?;

    let pristine_bytes = client
        .storage()
        .at(best_block)
        .fetch(&pristine_code_address)
        .await?
        .map(|v| v.0);

    Ok(pristine_bytes)
}

/// Parse a contract account address from a storage key. Returns error if a key is
/// malformated.
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

/// Fetch all contract addresses from the storage using the provided client and count of
/// requested elements starting from an optional address
pub async fn fetch_all_contracts(
    client: &Client,
    rpc: &LegacyRpcMethods<DefaultConfig>,
) -> Result<Vec<AccountId32>> {
    let root_key = api::storage()
        .contracts()
        .contract_info_of_iter()
        .to_root_bytes();

    let best_block = get_best_block(rpc).await?;
    let mut keys = client
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

#[cfg(test)]
mod tests {
    use super::*;
    use scale::Encode;
    use scale_info::{
        IntoPortable,
        Path,
    };
    use subxt::metadata::{
        types::Metadata,
        DecodeWithMetadata,
    };

    /// Find the type index in the metadata.
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
    fn deposit_decode_works() {
        #[subxt::subxt(runtime_metadata_path = "src/runtime_api/metadata_V11.scale")]
        mod api_v11 {}

        use api_v11::runtime_types::{
            bounded_collections::bounded_vec::BoundedVec,
            pallet_contracts::storage::{
                ContractInfo,
                DepositAccount,
            },
        };

        let metadata_bytes = std::fs::read("src/runtime_api/metadata_V11.scale").unwrap();
        let metadata = Metadata::decode(&mut &*metadata_bytes).unwrap();
        let contract_info_type_id = get_metadata_type_index(
            "ContractInfo",
            "pallet_contracts::storage",
            &metadata,
        )
        .expect("the contract info type must be present in the metadata");

        let contract_info = ContractInfo {
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
            &mut &*contract_info.encode(),
            contract_info_type_id as u32,
            &metadata.into(),
        )
        .expect("the contract info must be decoded");

        let deposit = get_deposit_account_id(&contract_info_thunk)
            .expect("the deposit account must be decoded from contract info");

        assert_eq!(deposit, contract_info.deposit_account.0);
    }

    #[test]
    fn deposit_decode_fails() {
        #[subxt::subxt(runtime_metadata_path = "src/runtime_api/metadata.scale")]
        mod api_v15 {}

        use api_v15::runtime_types::{
            bounded_collections::{
                bounded_btree_map::BoundedBTreeMap,
                bounded_vec::BoundedVec,
            },
            pallet_contracts::storage::ContractInfo,
        };

        let metadata_bytes = std::fs::read("src/runtime_api/metadata_V11.scale").unwrap();
        let metadata = Metadata::decode(&mut &*metadata_bytes).unwrap();
        let contract_info_type_id = get_metadata_type_index(
            "ContractInfo",
            "pallet_contracts::storage",
            &metadata,
        )
        .expect("the contract info type must be present in the metadata");

        let contract_info = ContractInfo {
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
            &mut &*contract_info.encode(),
            contract_info_type_id as u32,
            &metadata.into(),
        )
        .expect("the contract info must be decoded");

        let res = get_deposit_account_id(&contract_info_thunk)
            .expect_err("decoding the deposit account must fail");
        assert_eq!(
            res.to_string(),
            "Error at : Error decoding bytes given the type ID and registry provided: Not enough data to fill buffer"
        );
    }
}

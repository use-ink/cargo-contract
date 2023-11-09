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

use colored::Colorize;
use env_check::compare_node_env_with_contract;
use subxt::utils::AccountId32;

use anyhow::{
    anyhow,
    Context,
    Result,
};
use std::path::PathBuf;

use crate::runtime_api::api;
use contract_build::{
    CrateMetadata,
    DEFAULT_KEY_COL_WIDTH,
};
use scale::{
    Decode,
    Encode,
};
use subxt::{
    blocks,
    config,
    tx,
    Config,
    OnlineClient,
};
use subxt_signer::sr25519::Keypair;

use std::{
    option::Option,
    path::Path,
};
use subxt::backend::legacy::LegacyRpcMethods;

/// Fetch the contract info from the storage using the provided client.
pub async fn fetch_contract_info(
    contract: &AccountId32,
    rpc: &LegacyRpcMethods<DefaultConfig>,
    client: &Client,
) -> Result<Option<ContractInfo>> {
    let info_contract_call = api::storage().contracts().contract_info_of(contract);

    let best_block = get_best_block(rpc).await?;

    let contract_info_of = client
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

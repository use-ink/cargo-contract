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

mod balance;
mod call;
mod error;
mod events;
mod extrinsic_opts;
mod instantiate;
mod remove;
mod runtime_api;
mod upload;

#[cfg(test)]
#[cfg(feature = "integration-tests")]
mod integration_tests;

<<<<<<< HEAD
use subxt::{utils::AccountId32,
    ext::scale_value::{
            Composite,
            Value,
            ValueDef,
        },
};
=======
use colored::Colorize;
use subxt::utils::AccountId32;
>>>>>>> origin/master

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

pub use balance::{
    BalanceVariant,
    TokenMetadata,
};
pub use call::{
    CallCommandBuilder,
    CallExec,
    CallRequest,
};
use contract_metadata::ContractMetadata;
pub use contract_transcode::ContractMessageTranscoder;
pub use error::{
    ErrorVariant,
    GenericError,
};
pub use events::DisplayEvents;
pub use extrinsic_opts::{
    state,
    ExtrinsicOptsBuilder,
    Missing,
};
pub use instantiate::{
    Code,
    InstantiateArgs,
    InstantiateCommandBuilder,
    InstantiateDryRunResult,
    InstantiateExec,
    InstantiateExecResult,
};
pub use remove::{
    RemoveCommandBuilder,
    RemoveExec,
};

pub use subxt::PolkadotConfig as DefaultConfig;
pub use upload::{
    CodeUploadRequest,
    UploadCommandBuilder,
    UploadExec,
    UploadResult,
};

pub type Client = OnlineClient<DefaultConfig>;
pub type Balance = u128;
pub type CodeHash = <DefaultConfig as Config>::Hash;

use api::runtime_types::pallet_contracts::storage::ContractInfo;
type StorageVersion = u16;

/// Contract artifacts for use with extrinsic commands.
#[derive(Debug)]
pub struct ContractArtifacts {
    /// The original artifact path
    artifacts_path: PathBuf,
    /// The expected path of the file containing the contract metadata.
    metadata_path: PathBuf,
    /// The deserialized contract metadata if the expected metadata file exists.
    metadata: Option<ContractMetadata>,
    /// The Wasm code of the contract if available.
    pub code: Option<WasmCode>,
}

impl ContractArtifacts {
    /// Load contract artifacts.
    pub fn from_manifest_or_file(
        manifest_path: Option<&PathBuf>,
        file: Option<&PathBuf>,
    ) -> Result<ContractArtifacts> {
        let artifact_path = match (manifest_path, file) {
            (manifest_path, None) => {
                let crate_metadata = CrateMetadata::from_manifest_path(
                    manifest_path,
                    contract_build::Target::Wasm,
                )?;

                if crate_metadata.contract_bundle_path().exists() {
                    crate_metadata.contract_bundle_path()
                } else if crate_metadata.metadata_path().exists() {
                    crate_metadata.metadata_path()
                } else {
                    anyhow::bail!(
                        "Failed to find any contract artifacts in target directory. \n\
                        Run `cargo contract build --release` to generate the artifacts."
                    )
                }
            }
            (None, Some(artifact_file)) => artifact_file.clone(),
            (Some(_), Some(_)) => {
                anyhow::bail!("conflicting options: --manifest-path and --file")
            }
        };
        Self::from_artifact_path(artifact_path.as_path())
    }
    /// Given a contract artifact path, load the contract code and metadata where
    /// possible.
    fn from_artifact_path(path: &Path) -> Result<Self> {
        tracing::debug!("Loading contracts artifacts from `{}`", path.display());
        let (metadata_path, metadata, code) =
            match path.extension().and_then(|ext| ext.to_str()) {
                Some("contract") | Some("json") => {
                    let metadata = ContractMetadata::load(path)?;
                    let code = metadata.clone().source.wasm.map(|wasm| WasmCode(wasm.0));
                    (PathBuf::from(path), Some(metadata), code)
                }
                Some("wasm") => {
                    let file_name = path.file_stem()
                        .context("WASM bundle file has unreadable name")?
                        .to_str()
                        .context("Error parsing filename string")?;
                    let code = Some(WasmCode(std::fs::read(path)?));
                    let dir = path.parent().map_or_else(PathBuf::new, PathBuf::from);
                    let metadata_path = dir.join(format!("{file_name}.json"));
                    if !metadata_path.exists() {
                        (metadata_path, None, code)
                    } else {
                        let metadata = ContractMetadata::load(&metadata_path)?;
                        (metadata_path, Some(metadata), code)
                    }
                }
                Some(ext) => anyhow::bail!(
                    "Invalid artifact extension {ext}, expected `.contract`, `.json` or `.wasm`"
                ),
                None => {
                    anyhow::bail!(
                        "Artifact path has no extension, expected `.contract`, `.json`, or `.wasm`"
                    )
                }
            };

        if let Some(contract_metadata) = metadata.as_ref() {
            if let Err(e) = contract_metadata.check_ink_compatibility() {
                eprintln!("{} {}", "warning:".yellow().bold(), e.to_string().bold());
            }
        }
        Ok(Self {
            artifacts_path: path.into(),
            metadata_path,
            metadata,
            code,
        })
    }

    /// Get the path of the artifact file used to load the artifacts.
    pub fn artifact_path(&self) -> &Path {
        self.artifacts_path.as_path()
    }

    /// Get contract metadata, if available.
    ///
    /// ## Errors
    /// - No contract metadata could be found.
    /// - Invalid contract metadata.
    pub fn metadata(&self) -> Result<ContractMetadata> {
        self.metadata.clone().ok_or_else(|| {
            anyhow!(
                "No contract metadata found. Expected file {}",
                self.metadata_path.as_path().display()
            )
        })
    }

    /// Get the code hash from the contract metadata.
    pub fn code_hash(&self) -> Result<[u8; 32]> {
        let metadata = self.metadata()?;
        Ok(metadata.source.hash.0)
    }

    /// Construct a [`ContractMessageTranscoder`] from contract metadata.
    pub fn contract_transcoder(&self) -> Result<ContractMessageTranscoder> {
        let metadata = self.metadata()?;
        ContractMessageTranscoder::try_from(metadata)
            .context("Failed to deserialize ink project metadata from contract metadata")
    }
}

/// The Wasm code of a contract.
#[derive(Debug)]
pub struct WasmCode(Vec<u8>);

impl WasmCode {
    /// The hash of the contract code: uniquely identifies the contract code on-chain.
    pub fn code_hash(&self) -> [u8; 32] {
        contract_build::code_hash(&self.0)
    }
}

/// Get the account id from the Keypair
pub fn account_id(keypair: &Keypair) -> AccountId32 {
    subxt::tx::Signer::<DefaultConfig>::account_id(keypair)
}

/// Wait for the transaction to be included successfully into a block.
///
/// # Errors
///
/// If a runtime Module error occurs, this will only display the pallet and error indices.
/// Dynamic lookups of the actual error will be available once the following issue is
/// resolved: <https://github.com/paritytech/subxt/issues/443>.
///
/// # Finality
///
/// Currently this will report success once the transaction is included in a block. In the
/// future there could be a flag to wait for finality before reporting success.
async fn submit_extrinsic<T, Call, Signer>(
    client: &OnlineClient<T>,
    rpc: &LegacyRpcMethods<T>,
    call: &Call,
    signer: &Signer,
) -> core::result::Result<blocks::ExtrinsicEvents<T>, subxt::Error>
where
    T: Config,
    Call: tx::TxPayload,
    Signer: tx::Signer<T>,
    <T::ExtrinsicParams as config::ExtrinsicParams<T>>::OtherParams: Default,
{
    let account_id = Signer::account_id(signer);
    let account_nonce = get_account_nonce(client, rpc, &account_id).await?;

    client
        .tx()
        .create_signed_with_nonce(call, signer, account_nonce, Default::default())?
        .submit_and_watch()
        .await?
        .wait_for_in_block()
        .await?
        .wait_for_success()
        .await
}

/// Return the account nonce at the *best* block for an account ID.
async fn get_account_nonce<T>(
    client: &OnlineClient<T>,
    rpc: &LegacyRpcMethods<T>,
    account_id: &T::AccountId,
) -> core::result::Result<u64, subxt::Error>
where
    T: Config,
{
    let best_block = rpc
        .chain_get_block_hash(None)
        .await?
        .ok_or(subxt::Error::Other("Best block not found".into()))?;
    let account_nonce = client
        .blocks()
        .at(best_block)
        .await?
        .account_nonce(account_id)
        .await?;
    Ok(account_nonce)
}

async fn state_call<A: Encode, R: Decode>(
    rpc: &LegacyRpcMethods<DefaultConfig>,
    func: &str,
    args: A,
) -> Result<R> {
    let params = args.encode();
    let bytes = rpc.state_call(func, Some(&params), None).await?;
    Ok(R::decode(&mut bytes.as_ref())?)
}

/// Parse a hex encoded 32 byte hash. Returns error if not exactly 32 bytes.
pub fn parse_code_hash(input: &str) -> Result<<DefaultConfig as Config>::Hash> {
    let bytes = contract_build::util::decode_hex(input)?;
    if bytes.len() != 32 {
        anyhow::bail!("Code hash should be 32 bytes in length")
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr.into())
}

/// Calculate the total contract deposit.
async fn get_contract_total_deposit(
    storage_base_deposit: Balance,
    storage_item_deposit: Balance,
    storage_byte_deposit: Balance,
    client: &Client,
) -> Result<Balance> {
    let contract_pallet_version = fetch_contracts_pallet_version(client).await?;
    let mut contract_deposit = storage_base_deposit
        .saturating_add(storage_item_deposit)
        .saturating_add(storage_byte_deposit);

    // From contracts pallet version 10 deposit calculation has changed.
    if contract_pallet_version >= 10 {
        let existential_deposit_address =
            api::constants().balances().existential_deposit();
        let existential_deposit = client.constants().at(&existential_deposit_address)?;
        contract_deposit = contract_deposit.saturating_sub(existential_deposit);
    }
    Ok(contract_deposit)
}

async fn get_contract_total_deposit2(deposit_account: &AccountId32, pallet_contracts_version: StorageVersion) -> Result<Balance> {
    Ok(1)
}

/// Try to extract the given field from a dynamic [`Value`].
///
/// Returns `Err` if:
///   - The value is not a [`Value::Composite`] with [`Composite::Named`] fields
///   - The value does not contain a field with the given name.
fn get_composite_field_value<'a, T, C>(
    value: &'a Value<T>,
    field_name: &str,
) -> Result<&'a Value<T>>
where
    C: subxt::Config,
{
    if let ValueDef::Composite(Composite::Named(fields)) = &value.value {
        let (_, field) = fields
            .iter()
            .find(|(name, _)| name == field_name)
            .ok_or_else(|| {
                anyhow!("No field named '{}' found", field_name)
            })?;
        Ok(field)
    } else {
        anyhow::bail!("Expected a composite type with named fields")
    }
}

fn getDepositAccount(contract_info: &ContractInfo) -> Option<AccountId32> {
    //let deposit_account = get_composite_field_value::<_, DefaultConfig>(contract_info, "deposit_account")?;
    None
}

/// Fetch the contracts pallet version from the storage using the provided client.
async fn fetch_contracts_pallet_version(client: &Client) -> Result<StorageVersion> {
    let hash_pallet = hashing::twox_128(b"Contracts");
    let hash_version = hashing::twox_128(b":__STORAGE_VERSION__:");
    let key = [hash_pallet, hash_version].concat();

    let version = client
        .rpc()
        .storage(key.as_slice(), None)
        .await?
        .ok_or(anyhow!("Failed to get storage version of contracts pallet"))?
        .0;
    let version = StorageVersion::decode(&mut version.as_slice())?;
    println!("pallet version {}", version);
    Ok(version)
}

async fn get_account_balance(account: &AccountId32, client: &Client) -> Result<(Balance, Balance)> {
    let storage_query = api::storage().system().account(account);
    let result = client
        .storage()
        .at_latest()
        .await?
        .fetch(&storage_query)
        .await?;
    let result: api::runtime_types::frame_system::AccountInfo<u32, api::runtime_types::pallet_balances::types::AccountData<u128>> = result.unwrap();
    println!(
        "contract balance: free: {} reserved: {}",
        result.data.free, result.data.reserved
    );
    let ex = api::constants().balances().existential_deposit();
    let value = client.constants().at(&ex)?;
    println!("existential deposit: {}", value);

    Ok((result.data.free, result.data.reserved))
}

/// Fetch the hash of the *best* block (included but not guaranteed to be finalized).
async fn get_best_block(
    rpc: &LegacyRpcMethods<DefaultConfig>,
) -> core::result::Result<<DefaultConfig as Config>::Hash, subxt::Error> {
    rpc.chain_get_block_hash(None)
        .await?
        .ok_or(subxt::Error::Other("Best block not found".into()))
}

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
    println!("Contract account balance");
    get_account_balance(contract, client).await?;
    match contract_info_of {
        Some(info_result) => {
            let convert_trie_id = hex::encode(info_result.trie_id.0);


            println!("Deposit account balance");
            get_account_balance(&info_result.deposit_account.0, client).await?;
            println!("base: {}, item: {}, byte: {}", info_result.storage_base_deposit, info_result.storage_item_deposit, info_result.storage_byte_deposit);
            let total_deposit = get_contract_total_deposit(
                info_result.storage_base_deposit,
                info_result.storage_item_deposit,
                info_result.storage_byte_deposit,
                client,
            )
            .await?;



            get_contract_total_deposit2(contract, ).await?;

            Ok(Some(ContractInfo {
                trie_id: convert_trie_id,
                code_hash: info_result.code_hash,
                storage_items: info_result.storage_items,
                storage_item_deposit: info_result.storage_item_deposit,
                storage_total_deposit: total_deposit,
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
    pub fn storage_item_deposit(&self) -> Balance {
        self.storage_item_deposit
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
    // Twox64Concat(AccountId).
    let mut account = storage_contract_account_key
        .get(storage_contract_root_key_len + 8..)
        .ok_or(anyhow!("Unexpected storage key size"))?;
    AccountId32::decode(&mut account)
        .map_err(|err| anyhow!("AccountId deserialization error: {}", err))
}

/// Fetch all contract addresses from the storage using the provided client and count of
/// requested elements starting from an optional address.
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

// Converts a Url into a String representation without excluding the default port.
pub fn url_to_string(url: &url::Url) -> String {
    match (url.port(), url.port_or_known_default()) {
        (None, Some(port)) => {
            format!(
                "{}:{port}{}",
                &url[..url::Position::AfterHost],
                &url[url::Position::BeforePath..]
            )
            .to_string()
        }
        _ => url.to_string(),
    }
}

/// Copy of `pallet_contracts_primitives::StorageDeposit` which implements `Serialize`,
/// required for json output.
#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, serde::Serialize)]
pub enum StorageDeposit {
    /// The transaction reduced storage consumption.
    ///
    /// This means that the specified amount of balance was transferred from the involved
    /// contracts to the call origin.
    Refund(Balance),
    /// The transaction increased overall storage usage.
    ///
    /// This means that the specified amount of balance was transferred from the call
    /// origin to the contracts involved.
    Charge(Balance),
}

impl From<&pallet_contracts_primitives::StorageDeposit<Balance>> for StorageDeposit {
    fn from(deposit: &pallet_contracts_primitives::StorageDeposit<Balance>) -> Self {
        match deposit {
            pallet_contracts_primitives::StorageDeposit::Refund(balance) => {
                Self::Refund(*balance)
            }
            pallet_contracts_primitives::StorageDeposit::Charge(balance) => {
                Self::Charge(*balance)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_code_hash_works() {
        // with 0x prefix
        assert!(parse_code_hash(
            "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
        )
        .is_ok());
        // without 0x prefix
        assert!(parse_code_hash(
            "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
        )
        .is_ok())
    }

    #[test]
    fn url_to_string_works() {
        // with custom port
        let url = url::Url::parse("ws://127.0.0.1:9944").unwrap();
        assert_eq!(url_to_string(&url), "ws://127.0.0.1:9944/");

        // with default port
        let url = url::Url::parse("wss://127.0.0.1:443").unwrap();
        assert_eq!(url_to_string(&url), "wss://127.0.0.1:443/");

        // with default port and path
        let url = url::Url::parse("wss://127.0.0.1:443/test/1").unwrap();
        assert_eq!(url_to_string(&url), "wss://127.0.0.1:443/test/1");

        // with default port and domain
        let url = url::Url::parse("wss://test.io:443").unwrap();
        assert_eq!(url_to_string(&url), "wss://test.io:443/");

        // with default port, domain and path
        let url = url::Url::parse("wss://test.io/test/1").unwrap();
        assert_eq!(url_to_string(&url), "wss://test.io:443/test/1");
    }
}

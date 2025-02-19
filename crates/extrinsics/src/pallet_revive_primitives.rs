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

use pallet_revive_uapi::ReturnFlags;
use scale::{
    Decode,
    Encode,
    MaxEncodedLen,
};
use scale_info::TypeInfo;
use sp_runtime::{
    DispatchError,
    RuntimeDebug,
};
use sp_weights::Weight;
use subxt::utils::{
    H160,
    H256,
};

// todo reference `pallet-revive::ContractResult` directly
/// Copied from `pallet_revive`, required for RPC calls.
///
/// Result type of a `bare_call` or `bare_instantiate` call as well as
/// `ContractsApi::call` and `ContractsApi::instantiate`.
///
/// It contains the execution result together with some auxiliary information.
///
/// # Note
///
/// It has been extended to include `events` at the end of the struct while not bumping
/// the `ContractsApi` version. Therefore when SCALE decoding a `ContractResult` its
/// trailing data should be ignored to avoid any potential compatibility issues.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct ContractResult<R, Balance> {
    /// How much weight was consumed during execution.
    pub gas_consumed: Weight,
    /// How much weight is required as gas limit in order to execute this call.
    ///
    /// This value should be used to determine the weight limit for on-chain execution.
    ///
    /// # Note
    ///
    /// This can only different from [`Self::gas_consumed`] when weight pre charging
    /// is used. Currently, only `seal_call_runtime` makes use of pre charging.
    /// Additionally, any `seal_call` or `seal_instantiate` makes use of pre-charging
    /// when a non-zero `gas_limit` argument is supplied.
    pub gas_required: Weight,
    /// How much balance was paid by the origin into the contract's deposit account in
    /// order to pay for storage.
    ///
    /// The storage deposit is never actually charged from the origin in case of
    /// [`Self::result`] is `Err`. This is because on error all storage changes are
    /// rolled back including the payment of the deposit.
    pub storage_deposit: StorageDeposit<Balance>,
    /// The execution result of the code.
    pub result: R,
}

/// Result type of a `bare_call` call, as well as `ContractsApi::call`.
pub type ContractExecResult<Balance> =
    ContractResult<Result<ExecReturnValue, DispatchError>, Balance>;

/// Result type of a `bare_instantiate` call, as well as `ContractsApi::instantiate`.
pub type ContractInstantiateResult<Balance> =
    ContractResult<Result<InstantiateReturnValue, DispatchError>, Balance>;

/// Result type of a `bare_code_upload` call.
pub type CodeUploadResult<Balance> =
    Result<CodeUploadReturnValue<Balance>, DispatchError>;

/// Result type of a `get_storage` call.
pub type GetStorageResult = Result<Option<Vec<u8>>, ContractAccessError>;

/// The possible errors that can happen querying the storage of a contract.
#[derive(
    Copy, Clone, Eq, PartialEq, Encode, Decode, MaxEncodedLen, RuntimeDebug, TypeInfo,
)]
pub enum ContractAccessError {
    /// The given address doesn't point to a contract.
    DoesntExist,
    /// Storage key cannot be decoded from the provided input data.
    KeyDecodingFailed,
    /// Storage is migrating. Try again later.
    MigrationInProgress,
}

/// Output of a contract call or instantiation which ran to completion.
#[derive(Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct ExecReturnValue {
    /// Flags passed along by `seal_return`. Empty when `seal_return` was never called.
    pub flags: ReturnFlags,
    /// Buffer passed along by `seal_return`. Empty when `seal_return` was never called.
    pub data: Vec<u8>,
}

impl ExecReturnValue {
    /// The contract did revert all storage changes.
    pub fn did_revert(&self) -> bool {
        self.flags.contains(ReturnFlags::REVERT)
    }
}

/// The result of a successful contract instantiation.
#[derive(Clone, PartialEq, Eq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct InstantiateReturnValue {
    /// The output of the called constructor.
    pub result: ExecReturnValue,
    /// The account id of the new contract.
    pub account_id: H160,
}

/// The result of successfully uploading a contract.
#[derive(Clone, PartialEq, Eq, Encode, Decode, MaxEncodedLen, RuntimeDebug, TypeInfo)]
pub struct CodeUploadReturnValue<Balance> {
    /// The key under which the new code is stored.
    pub code_hash: H256,
    /// The deposit that was reserved at the caller. Is zero when the code already
    /// existed.
    pub deposit: Balance,
}

/// Reference to an existing code hash or a new contract binary.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo)]
pub enum Code<Hash> {
    /// Bytecode of a contract.
    Upload(Vec<u8>),
    /// The code hash of an on-chain contract binary.
    Existing(Hash),
}

/// The amount of balance that was either charged or refunded in order to pay for storage.
#[derive(
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Encode,
    Decode,
    MaxEncodedLen,
    RuntimeDebug,
    TypeInfo,
    serde::Serialize,
)]
pub enum StorageDeposit<Balance> {
    /// The transaction reduced storage consumption.
    ///
    /// This means that the specified amount of balance was transferred from the involved
    /// deposit accounts to the origin.
    Refund(Balance),
    /// The transaction increased storage consumption.
    ///
    /// This means that the specified amount of balance was transferred from the origin
    /// to the involved deposit accounts.
    Charge(Balance),
}

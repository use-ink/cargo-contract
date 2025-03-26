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

use crate::ContractBinary;
use subxt::{
    ext::scale_encode::EncodeAsType,
    utils::{
        H160,
        H256,
    },
};

/// Copied from `sp_weight` to additionally implement `scale_encode::EncodeAsType`.
#[derive(Debug, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub(crate) struct Weight {
    #[codec(compact)]
    /// The weight of computational time used based on some reference hardware.
    ref_time: u64,
    #[codec(compact)]
    /// The weight of storage space used by proof of validity.
    proof_size: u64,
}

impl From<sp_weights::Weight> for Weight {
    fn from(weight: sp_weights::Weight) -> Self {
        Self {
            ref_time: weight.ref_time(),
            proof_size: weight.proof_size(),
        }
    }
}

impl core::fmt::Display for Weight {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Weight(ref_time: {}, proof_size: {})",
            self.ref_time, self.proof_size
        )
    }
}

/// A raw call to `pallet-contracts`'s `remove_code`.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub(crate) struct RemoveCode<Hash> {
    code_hash: Hash,
}

impl<Hash> RemoveCode<Hash> {
    pub fn new(code_hash: Hash) -> Self {
        Self { code_hash }
    }

    pub fn build(self) -> subxt::tx::DefaultPayload<Self> {
        subxt::tx::DefaultPayload::new("Revive", "remove_code", self)
    }
}

/// A raw call to `pallet-contracts`'s `upload_code`.
#[derive(Debug, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct UploadCode<Balance> {
    code: Vec<u8>,
    storage_deposit_limit: Balance,
}

impl<Balance> UploadCode<Balance> {
    pub fn new(code: ContractBinary, storage_deposit_limit: Balance) -> Self {
        Self {
            code: code.0,
            storage_deposit_limit,
        }
    }

    pub fn build(self) -> subxt::tx::DefaultPayload<Self> {
        subxt::tx::DefaultPayload::new("Revive", "upload_code", self)
    }
}

/// A raw call to `pallet-contracts`'s `instantiate_with_code`.
#[derive(Debug, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct InstantiateWithCode<Balance> {
    #[codec(compact)]
    value: Balance,
    gas_limit: Weight,
    #[codec(compact)]
    storage_deposit_limit: Balance,
    code: Vec<u8>,
    data: Vec<u8>,
    salt: Option<Vec<u8>>,
}

impl<Balance> InstantiateWithCode<Balance> {
    pub fn new(
        value: Balance,
        gas_limit: sp_weights::Weight,
        storage_deposit_limit: Balance,
        code: Vec<u8>,
        data: Vec<u8>,
        salt: Option<Vec<u8>>,
    ) -> Self {
        Self {
            value,
            gas_limit: gas_limit.into(),
            storage_deposit_limit,
            code,
            data,
            salt,
        }
    }

    pub fn build(self) -> subxt::tx::DefaultPayload<Self> {
        subxt::tx::DefaultPayload::new("Revive", "instantiate_with_code", self)
    }
}

/// A raw call to `pallet-contracts`'s `instantiate_with_code_hash`.
#[derive(Debug, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct Instantiate<Balance> {
    #[codec(compact)]
    value: Balance,
    gas_limit: Weight,
    #[codec(compact)]
    storage_deposit_limit: Balance,
    code_hash: H256,
    data: Vec<u8>,
    salt: Option<[u8; 32]>,
}

impl<Balance> Instantiate<Balance> {
    pub fn new(
        value: Balance,
        gas_limit: sp_weights::Weight,
        storage_deposit_limit: Balance,
        code_hash: H256,
        data: Vec<u8>,
        salt: Option<[u8; 32]>,
    ) -> Self {
        Self {
            value,
            gas_limit: gas_limit.into(),
            storage_deposit_limit,
            code_hash,
            data,
            salt,
        }
    }

    pub fn build(self) -> subxt::tx::DefaultPayload<Self> {
        subxt::tx::DefaultPayload::new("Revive", "instantiate", self)
    }
}

/// A raw call to `pallet-contracts`'s `call`.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct Call<Balance> {
    dest: H160,
    #[codec(compact)]
    value: Balance,
    gas_limit: Weight,
    storage_deposit_limit: Balance,
    data: Vec<u8>,
}

impl<Balance> Call<Balance> {
    pub fn new(
        dest: H160,
        value: Balance,
        gas_limit: sp_weights::Weight,
        storage_deposit_limit: Balance,
        data: Vec<u8>,
    ) -> Self {
        Self {
            dest,
            value,
            gas_limit: gas_limit.into(),
            storage_deposit_limit,
            data,
        }
    }

    pub fn build(self) -> subxt::tx::DefaultPayload<Self> {
        subxt::tx::DefaultPayload::new("Revive", "call", self)
    }
}

/// A raw call to `pallet-contracts`'s `instantiate_with_code_hash`.
#[derive(Debug, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub(crate) struct MapAccount {}

impl MapAccount {
    pub fn new() -> Self {
        Self {}
    }

    pub fn build(self) -> subxt::tx::DefaultPayload<Self> {
        subxt::tx::DefaultPayload::new("Revive", "map_account", self)
    }
}

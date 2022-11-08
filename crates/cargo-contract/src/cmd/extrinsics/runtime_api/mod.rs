// Copyright 2018-2020 Parity Technologies (UK) Ltd.
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

#[subxt::subxt(
    runtime_metadata_path = "src/cmd/extrinsics/runtime_api/contracts_runtime.scale"
)]
pub mod api {
    #[subxt(substitute_type = "sp_weights::weight_v2::Weight")]
    use crate::cmd::extrinsics::runtime_api::Weight;
}

/// Copy of the `weight_v2::Weight` type defined in substrate.
///
/// Allows for local trait and inherent impls.
#[derive(scale::Encode, scale::Decode, Clone, Copy, Debug)]
pub struct Weight {
    #[codec(compact)]
    /// The weight of computational time used based on some reference hardware.
    pub ref_time: u64,
    #[codec(compact)]
    /// The weight of storage space used by proof of validity.
    pub proof_size: u64,
}

impl ToString for Weight {
    fn to_string(&self) -> String {
        self.ref_time.to_string()
    }
}

impl Weight {
    pub fn from_ref_time(ref_time: u64) -> Self {
        Self { ref_time, proof_size: 0 }
    }
}

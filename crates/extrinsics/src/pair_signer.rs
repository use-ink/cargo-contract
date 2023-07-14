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

use crate::Config;
use sp_core::Pair as PairT;
use sp_runtime::{
    traits::{IdentifyAccount, Verify},
    MultiSignature as SpMultiSignature,
};

/// A [`Signer`] implementation that can be constructed from an [`sp_core::Pair`].
#[derive(Clone)]
pub struct PairSigner<T: Config, Pair> {
    account_id: T::AccountId,
    signer: Pair,
}

impl<T, Pair> PairSigner<T, Pair>
where
    T: Config,
    Pair: PairT,
    <SpMultiSignature as Verify>::Signer: From<Pair::Public>,
    T::AccountId: From<account::AccountId20>,
{
    /// Creates a new [`Signer`] from an [`sp_core::Pair`].
    pub fn new(signer: Pair) -> Self {
        // todo: convert the signer to the account id, this currently is `AccountId32`
        let account_id =
            <SpMultiSignature as Verify>::Signer::from(signer.public()).into_account();
        Self {
            account_id: account_id.into(),
            signer,
        }
    }

    /// Returns the [`sp_core::Pair`] implementation used to construct this.
    pub fn signer(&self) -> &Pair {
        &self.signer
    }

    /// Return the account ID.
    pub fn account_id(&self) -> &T::AccountId {
        &self.account_id
    }
}

impl<T, Pair> subxt::tx::Signer<T> for PairSigner<T, Pair>
    where
        T: Config,
        Pair: PairT,
        Pair::Signature: Into<T::Signature>,
{
    fn account_id(&self) -> &T::AccountId {
        &self.account_id
    }

    fn address(&self) -> T::Address {
        self.account_id.clone().into()
    }

    fn sign(&self, signer_payload: &[u8]) -> T::Signature {
        self.signer.sign(signer_payload).into()
    }
}
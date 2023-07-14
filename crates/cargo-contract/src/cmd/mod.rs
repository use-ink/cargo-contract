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

pub mod build;
pub mod decode;
pub mod encode;
pub mod info;

pub(crate) use self::{
    build::{
        BuildCommand,
        CheckCommand,
    },
    decode::DecodeCommand,
    info::InfoCommand,
};

pub(crate) use contract_extrinsics::{
    CallCommand,
    ErrorVariant,
    InstantiateCommand,
    RemoveCommand,
    UploadCommand,
};

use subxt::{
    Config,
    OnlineClient,
    // We bring this one in so that we can extend and override its types.
    SubstrateConfig, config::extrinsic_params::BaseExtrinsicParams,
};

type Client = OnlineClient<DefaultConfig>;
type Balance = u128;
type CodeHash = <DefaultConfig as Config>::Hash;

// Here is a core change. Instead of using the Polkadot config,
// We create our own that has the ethereum-style signature and accounts
// pub use subxt::PolkadotConfig as DefaultConfig;
pub use AcademyPowConfig as DefaultConfig;

/// A runtime configuration for the academy pow chain.
/// This thing is not meant to be instantiated; it is just a collection of types.
pub enum AcademyPowConfig{}
impl subxt::Config for DefaultConfig {
    type Index = <SubstrateConfig as Config>::Index;
    type Hash = <SubstrateConfig as Config>::Hash;
    type AccountId = account::AccountId20;
    type Address = account::AccountId20;
    type Signature = account::EthereumSignature;
    type Hasher = <SubstrateConfig as Config>::Hasher;
    type Header = <SubstrateConfig as Config>::Header;
    type ExtrinsicParams = AcademyPowExtrinsicParams<Self>;
}

/// A struct representing the signed extra and additional parameters required
/// to construct a transaction for an academy pow node. This is actually a direct copy
/// of PolkadotExtrinsicParams, but I wanted to make it more clear that it is for the plain tip.
pub type AcademyPowExtrinsicParams<T> = BaseExtrinsicParams<T, subxt::config::polkadot::PlainTip>;
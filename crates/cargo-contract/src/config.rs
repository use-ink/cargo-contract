// Copyright (C) Parity Technologies (UK) Ltd.
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

use contract_transcode::{
    AccountId20,
    EthereumSignature,
};
use ink_env::{
    DefaultEnvironment,
    Environment,
};
use subxt::{
    config::{
        DefaultExtrinsicParams,
        PolkadotExtrinsicParams,
    },
    utils::MultiAddress,
    Config,
    SubstrateConfig,
    tx::Signer as SignerT
};
use subxt_signer::sr25519::Keypair;

/// A runtime configuration for the Ethereum based chain like Moonbeam.
/// This thing is not meant to be instantiated; it is just a collection of types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EthereumBaseConfig {}
impl Config for EthereumBaseConfig {
    type Hash = <SubstrateConfig as Config>::Hash;
    type AccountId = AccountId20;
    type Address = AccountId20;
    type Signature = EthereumSignature;
    type Hasher = <SubstrateConfig as Config>::Hasher;
    type Header = <SubstrateConfig as Config>::Header;
    type ExtrinsicParams = DefaultExtrinsicParams<Self>;
    type AssetId = <SubstrateConfig as Config>::AssetId;
}

impl Environment for EthereumBaseConfig {
    const MAX_EVENT_TOPICS: usize = <DefaultEnvironment as Environment>::MAX_EVENT_TOPICS;
    type AccountId = <DefaultEnvironment as Environment>::AccountId;
    type Balance = <DefaultEnvironment as Environment>::Balance;
    type Hash = <DefaultEnvironment as Environment>::Hash;
    type Timestamp = <DefaultEnvironment as Environment>::Timestamp;
    type BlockNumber = <DefaultEnvironment as Environment>::BlockNumber;
    type ChainExtension = <DefaultEnvironment as Environment>::ChainExtension;
}

/// A runtime configuration for the Polkadot based chain.
// /// This thing is not meant to be instantiated; it is just a collection of types.
#[derive(Debug, Clone, PartialEq, Eq)]

pub enum PolkadotBaseConfig {}
impl Config for PolkadotBaseConfig {
    type Hash = <SubstrateConfig as Config>::Hash;
    type AccountId = <SubstrateConfig as Config>::AccountId;
    type Address = MultiAddress<Self::AccountId, ()>;
    type Signature = <SubstrateConfig as Config>::Signature;
    type Hasher = <SubstrateConfig as Config>::Hasher;
    type Header = <SubstrateConfig as Config>::Header;
    type ExtrinsicParams = PolkadotExtrinsicParams<Self>;
    type AssetId = u32;
}

impl Environment for PolkadotBaseConfig {
    const MAX_EVENT_TOPICS: usize = <DefaultEnvironment as Environment>::MAX_EVENT_TOPICS;
    type AccountId = <DefaultEnvironment as Environment>::AccountId;
    type Balance = <DefaultEnvironment as Environment>::Balance;
    type Hash = <DefaultEnvironment as Environment>::Hash;
    type Timestamp = <DefaultEnvironment as Environment>::Timestamp;
    type BlockNumber = <DefaultEnvironment as Environment>::BlockNumber;
    type ChainExtension = <DefaultEnvironment as Environment>::ChainExtension;
}

pub trait SignerConfig<C: Config> 
where
    Self: Clone,
{
    type Signer: Clone + SignerT<C>;
}

impl SignerConfig<Self> for PolkadotBaseConfig {
    type Signer = Keypair;
}

// impl SignerConfig<EthereumBaseConfig> for EthereumBaseConfig {
//      type Signer = ();
// }

pub enum ChainConfig {
    PolkadotBaseConfig,
   // EthereumBaseConfig,
}

pub fn select_config(config: &str) -> ChainConfig {
    ChainConfig::PolkadotBaseConfig
}

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

use ink_env::{
    DefaultEnvironment,
    Environment,
};
use sp_core::Pair;
use std::{
    fmt::Debug,
    str::FromStr,
};
use subxt::{
    config::{
        PolkadotExtrinsicParams,
        SubstrateExtrinsicParams,
    },
    tx::Signer as SignerT,
    utils::{
        AccountId32,
        MultiSignature,
    },
    Config,
    PolkadotConfig,
    SubstrateConfig,
};

/// Configuration for signer
pub trait SignerConfig<C: Config + Environment> {
    type Signer: SignerT<C> + FromStr + Clone;
}

/// A runtime configuration for the ecdsa test chain.
/// This thing is not meant to be instantiated; it is just a collection of types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ecdsachain {}

impl Config for Ecdsachain {
    type AccountId = <SubstrateConfig as Config>::AccountId;
    type Address = <SubstrateConfig as Config>::Address;
    type Signature = <SubstrateConfig as Config>::Signature;
    type Hasher = <SubstrateConfig as Config>::Hasher;
    type Header = <SubstrateConfig as Config>::Header;
    type ExtrinsicParams = SubstrateExtrinsicParams<Self>;
    type AssetId = <SubstrateConfig as Config>::AssetId;
}

impl Environment for Ecdsachain {
    const MAX_EVENT_TOPICS: usize = <DefaultEnvironment as Environment>::MAX_EVENT_TOPICS;
    type AccountId = <DefaultEnvironment as Environment>::AccountId;
    type Balance = <DefaultEnvironment as Environment>::Balance;
    type Hash = <DefaultEnvironment as Environment>::Hash;
    type Timestamp = <DefaultEnvironment as Environment>::Timestamp;
    type BlockNumber = <DefaultEnvironment as Environment>::BlockNumber;
    type ChainExtension = <DefaultEnvironment as Environment>::ChainExtension;
    type EventRecord = ();
}

impl SignerConfig<Self> for Ecdsachain
where
    <Self as Config>::Signature: From<MultiSignature>,
{
    type Signer = SignerEcdsa<Self>;
}

/// A runtime configuration for the Substrate-based chain.
/// This thing is not meant to be instantiated; it is just a collection of types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Substrate {}

impl Config for Substrate {
    type AccountId = <SubstrateConfig as Config>::AccountId;
    type Address = <SubstrateConfig as Config>::Address;
    type Signature = <SubstrateConfig as Config>::Signature;
    type Hasher = <SubstrateConfig as Config>::Hasher;
    type Header = <SubstrateConfig as Config>::Header;
    type ExtrinsicParams = SubstrateExtrinsicParams<Self>;
    type AssetId = <SubstrateConfig as Config>::AssetId;
}

impl Environment for Substrate {
    const MAX_EVENT_TOPICS: usize = <DefaultEnvironment as Environment>::MAX_EVENT_TOPICS;
    type AccountId = <DefaultEnvironment as Environment>::AccountId;
    type Balance = <DefaultEnvironment as Environment>::Balance;
    type Hash = <DefaultEnvironment as Environment>::Hash;
    type Timestamp = <DefaultEnvironment as Environment>::Timestamp;
    type BlockNumber = <DefaultEnvironment as Environment>::BlockNumber;
    type ChainExtension = <DefaultEnvironment as Environment>::ChainExtension;
    type EventRecord = ();
}

impl SignerConfig<Self> for Substrate {
    type Signer = SignerSR25519<Self>;
}

/// A runtime configuration for the Polkadot-based chain.
/// This thing is not meant to be instantiated; it is just a collection of types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Polkadot {}

impl Config for Polkadot {
    type AccountId = <PolkadotConfig as Config>::AccountId;
    type Address = <PolkadotConfig as Config>::Address;
    type Signature = <PolkadotConfig as Config>::Signature;
    type Hasher = <PolkadotConfig as Config>::Hasher;
    type Header = <PolkadotConfig as Config>::Header;
    type ExtrinsicParams = PolkadotExtrinsicParams<Self>;
    type AssetId = <PolkadotConfig as Config>::AssetId;
}

impl Environment for Polkadot {
    const MAX_EVENT_TOPICS: usize = <DefaultEnvironment as Environment>::MAX_EVENT_TOPICS;
    type AccountId = <DefaultEnvironment as Environment>::AccountId;
    type Balance = <DefaultEnvironment as Environment>::Balance;
    type Hash = <DefaultEnvironment as Environment>::Hash;
    type Timestamp = <DefaultEnvironment as Environment>::Timestamp;
    type BlockNumber = <DefaultEnvironment as Environment>::BlockNumber;
    type ChainExtension = <DefaultEnvironment as Environment>::ChainExtension;
    type EventRecord = ();
}

impl SignerConfig<Self> for Polkadot {
    type Signer = SignerSR25519<Self>;
}

/// Struct representing the implementation of the sr25519 signer
#[derive(Clone)]
pub struct SignerSR25519<C: Config> {
    account_id: <C as Config>::AccountId,
    signer: sp_core::sr25519::Pair,
}

impl<C: Config<AccountId: From<AccountId32>>> SignerSR25519<C> {
    /// Creates a new [`SignerSR25519`] from a [`sp_core::sr25519::Pair`].
    pub fn new(signer: sp_core::sr25519::Pair) -> Self {
        Self {
            account_id: AccountId32(signer.public().0).into(),
            signer,
        }
    }
}

impl<C: Config<AccountId: From<AccountId32>>> FromStr for SignerSR25519<C> {
    type Err = anyhow::Error;

    /// Attempts to parse the Signer suri string
    fn from_str(input: &str) -> Result<SignerSR25519<C>, Self::Err> {
        let keypair = sp_core::sr25519::Pair::from_string(input, None)?;
        Ok(Self::new(keypair))
    }
}

impl<C: Config<Signature: From<MultiSignature>>> SignerT<C> for SignerSR25519<C> {
    fn account_id(&self) -> <C as Config>::AccountId {
        self.account_id.clone()
    }

    fn sign(&self, signer_payload: &[u8]) -> C::Signature {
        MultiSignature::Sr25519(self.signer.sign(signer_payload).0).into()
    }
}

/// Struct representing the implementation of the ecdsa signer
#[derive(Clone)]
pub struct SignerEcdsa<C: Config> {
    account_id: <C as Config>::AccountId,
    signer: sp_core::ecdsa::Pair,
}

impl<C: Config<AccountId: From<AccountId32>>> SignerEcdsa<C> {
    /// Creates a new [`SignerEcdsa`] from a [`sp_core::ecdsa::Pair`].
    pub fn new(signer: sp_core::ecdsa::Pair) -> Self {
        Self {
            account_id: AccountId32(sp_core::blake2_256(signer.public().as_ref())).into(),
            signer,
        }
    }
}

impl<C: Config<AccountId: From<AccountId32>>> FromStr for SignerEcdsa<C> {
    type Err = anyhow::Error;

    /// Attempts to parse the Signer suri string
    fn from_str(input: &str) -> Result<SignerEcdsa<C>, Self::Err> {
        let keypair = sp_core::ecdsa::Pair::from_string(input, None)?;
        Ok(Self::new(keypair))
    }
}

impl<C: Config<Signature: From<MultiSignature>>> SignerT<C> for SignerEcdsa<C> {
    fn account_id(&self) -> <C as Config>::AccountId {
        self.account_id.clone()
    }

    fn sign(&self, signer_payload: &[u8]) -> C::Signature {
        MultiSignature::Ecdsa(self.signer.sign(signer_payload).0).into()
    }
}

#[macro_export]
macro_rules! call_with_config_internal {
    ($obj:tt ,$function:tt, $config_name:expr, $( ($config_str:literal, $config_obj:ty) ),*) => {
        match $config_name {
            $(
                $config_str => $obj.$function::<$config_obj>().await,
            )*
            _ => {
              let configs = vec![$($config_str),*].iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(", ");
                Err(ErrorVariant::Generic(
                    contract_extrinsics::GenericError::from_message(
                        format!("Chain configuration {} not found, allowed configurations: {configs}", $config_name)
                )))
            },
        }
    };
}

/// Macro that allows calling the command member function with chain configuration
///
/// # Developer Note
///
/// In older Rust versions the macro `stringify!($crate::foo)` expanded to
/// `"$crate::foo"`. This behavior changed with https://github.com/rust-lang/rust/pull/125174,
/// `stringify!` expands to `"$crate :: foo"` now. To support both older and
/// newer Rust versions, our macro has to handle both cases, spaced and non-spaced.
///
/// # Known Limitation
///
///  The `$config_name:expr` has to be in the `$crate::cmd::config` crate and cannot
/// contain another `::` sub-path.
#[macro_export]
macro_rules! call_with_config {
    ($obj:tt, $function:ident, $config_name:expr) => {{
        assert!(
            !format!("{}", $config_name).contains("::"),
            "The supplied config name `{}` is not allowed to contain `::`.",
            $config_name
        );

        $crate::call_with_config_internal!(
            $obj,
            $function,
            $config_name,
            // All available chain configs need to be specified here
            ("Polkadot", $crate::cmd::config::Polkadot),
            ("Substrate", $crate::cmd::config::Substrate),
            ("Ecdsachain", $crate::cmd::config::Ecdsachain)
        )
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sr25519_signer_works() {
        let suri = "//Alice";
        let pair = sp_core::sr25519::Pair::from_string(suri, None).unwrap();
        let signer = SignerSR25519::<SubstrateConfig>::from_str(suri).unwrap();
        assert_eq!(signer.account_id, AccountId32(pair.public().0));
        assert_eq!(
            signer.account_id().to_string(),
            "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"
        );
        assert_eq!(signer.signer.public(), pair.public());
        assert!(matches!(signer.sign(b"test"), MultiSignature::Sr25519(_)))
    }

    #[test]
    fn ecdsa_signer_works() {
        let suri = "//Alice";
        let pair = sp_core::ecdsa::Pair::from_string(suri, None).unwrap();
        let signer = SignerEcdsa::<SubstrateConfig>::from_str(suri).unwrap();
        assert_eq!(
            signer.account_id,
            AccountId32(sp_core::blake2_256(&pair.public().0))
        );
        assert_eq!(
            signer.account_id().to_string(),
            "5C7C2Z5sWbytvHpuLTvzKunnnRwQxft1jiqrLD5rhucQ5S9X"
        );
        assert_eq!(signer.signer.public(), pair.public());
        assert!(matches!(signer.sign(b"test"), MultiSignature::Ecdsa(_)))
    }
}

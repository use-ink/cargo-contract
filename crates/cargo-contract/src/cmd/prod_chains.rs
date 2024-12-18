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

//! This file simply contains the end points of the production chains
//! We hard-code these values to ensure that a user uploads a verifiable bundle

use contract_extrinsics::url_to_string;
use std::str::FromStr;
use url::Url;

/// This macro generates enums with the pre-defined production chains and their respective
/// endpoints.
///
/// It also generates the required trait implementations.
macro_rules! define_chains {
    (
        $(#[$($attrs:tt)*])*
        pub enum $root:ident { $( $c:ident = ($ep:tt, $config:tt) ),* $(,)? }
    ) => {
        $(#[$($attrs)*])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum $root { $($c),* }

        impl $root {
            /// Returns the endpoint URL of a chain.
            pub fn url(&self) -> url::Url {
                match self {
                    $(
                        $root::$c => Url::parse($ep).expect("Incorrect Url format")
                    ),*
                }
            }

            /// Returns the config of a chain.
            pub fn config(&self) -> &str {
                match self {
                    $(
                        $root::$c => $config
                    ),*
                }
            }

            /// Returns the production chain.
            ///
            /// If the user specified the endpoint URL and config manually we'll attempt to
            /// convert it into one of the pre-defined production chains.
            pub fn from_parts(ep: &Url, config: &str) -> Option<Self> {
                match (url_to_string(ep).as_str(), config) {
                    $(
                        ($ep, $config) => Some($root::$c),
                    )*
                    _ => None
                }
            }
        }

       impl std::fmt::Display for $root {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(
                    $root::$c => f.write_str(stringify!($c))
                    ),*
                }
            }
        }

        impl FromStr for $root {
            type Err = anyhow::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $(
                        stringify!($c) => Ok($root::$c),
                    )*
                    _ => Err(anyhow::anyhow!("Unrecognised chain name"))
                }
            }
        }
    };
}

define_chains! {
    /// List of production chains where the contract can be deployed to.
    #[derive(clap::ValueEnum)]
    pub enum ProductionChain {
        AlephZero = ("wss://ws.azero.dev:443/", "Substrate"),
        Astar = ("wss://rpc.astar.network:443/", "Polkadot"),
        Shiden = ("wss://rpc.shiden.astar.network:443/", "Polkadot"),
        Krest = ("wss://wss-krest.peaq.network:443/", "Polkadot")
    }
}

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

//! This file simply contains the end points of the production chains
//! We hard-code these values to ensure that a user uploads a verifiable bundle

use std::str::FromStr;

/// Macro to generate enums with production chains and their respective endpoints
/// and generate required trait implementation
macro_rules! define_chains {
    (
        $(#[$($attrs:tt)*])*
        pub enum $root:ident { $( $c:ident = $ep:tt ),* $(,)? }
    ) => {
        $(#[$($attrs)*])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum $root { $($c),* }

        impl $root {
            /// Returns the endpoint URL of a chain.
            pub fn end_point(&self) -> &str {
                match self {
                    $(
                        $root::$c => $ep
                    ),*
                }
            }

            /// Returns the chain type from the endpoint URL
            pub fn chain_by_endpoint(ep: &str) -> Option<Self> {
                match ep {
                    $(
                        $ep => Some($root::$c),
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
    /// A list of all production chains where the contract can be deployed to.
    pub enum ProductionChain {
        AlephZero = "wss://ws.azero.dev:443",
        Astar = "wss://rpc.astar.network:443",
        Shiden = "wss://rpc.shiden.astar.network:443"
    }
}

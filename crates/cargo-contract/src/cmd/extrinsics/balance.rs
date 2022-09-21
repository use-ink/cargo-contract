// Copyright 2018-2022 Parity Technologies (UK) Ltd.
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

use std::fmt::Display;

use serde_json::json;

use super::{
    Balance,
    Client,
};

use anyhow::{
    Context,
    Ok,
    Result,
};

/// Represents different formats of a balance
#[derive(Debug, Clone)]
pub enum BalanceVariant {
    /// Default format: no symbol, no denomination
    Default(Balance),
    /// Denominated format: symbol and denomination are present
    Denominated(String),
}

#[derive(Debug, Clone)]
pub struct TokenMetadata {
    /// Number of denomination used for denomination
    pub denomination: u128,
    /// Token symbol
    pub symbol: String,
}

impl TokenMetadata {
    /// Query [TokenMetadata] through the node's RPC
    pub async fn query(client: &Client) -> Result<Self> {
        let sys_props = client.rpc().system_properties().await?;

        let default_decimals = json!(12);
        let default_units = json!("UNIT");
        let decimals = sys_props
            .get("tokenDecimal")
            .unwrap_or(&default_decimals)
            .as_u64()
            .context("error converting decimal to u64")?;
        let symbol = sys_props
            .get("tokenSymbol")
            .unwrap_or(&default_units)
            .as_str()
            .context("error converting symbol to string")?;
        let denomination: u128 = format!("1{}", "0".repeat(decimals as usize)).parse()?;
        Ok(Self {
            denomination,
            symbol: symbol.to_string(),
        })
    }
}

impl BalanceVariant {
    /// Converts BalanceVariant into Balance.
    ///
    /// Throws Error if [BalanceVariant::Denominated(String)] is in an incorrect format.
    pub fn denominate_balance(&self, token_metadata: &TokenMetadata) -> Result<Balance> {
        match self {
            BalanceVariant::Default(balance) => Ok(*balance),
            BalanceVariant::Denominated(input) => {
                if let Some(balance_str) =
                    input.strip_suffix(&format!("k{}", token_metadata.symbol))
                {
                    let balance: f64 = balance_str.parse()?;
                    Ok((balance * 1_000.0) as Balance * token_metadata.denomination)
                } else if let Some(balance_str) =
                    input.strip_suffix(&format!("M{}", token_metadata.symbol))
                {
                    let balance: f64 = balance_str.parse()?;
                    Ok((balance * 1_000_000.0) as Balance * token_metadata.denomination)
                } else if let Some(balance_str) =
                    input.strip_suffix(&format!("n{}", token_metadata.symbol))
                {
                    let balance: f64 = balance_str.parse()?;
                    Ok((balance * 1_000_000_000.0) as Balance)
                } else if let Some(balance_str) =
                    input.strip_suffix(&format!("μ{}", token_metadata.symbol))
                {
                    let balance: f64 = balance_str.parse()?;
                    Ok((balance * 1_000_000.0) as Balance)
                } else if let Some(balance_str) =
                    input.strip_suffix(&format!("m{}", token_metadata.symbol))
                {
                    let balance: f64 = balance_str.parse()?;
                    Ok((balance * 1_000.0) as Balance)
                } else if let Some(balance_str) =
                    input.strip_suffix(&token_metadata.symbol)
                {
                    let balance: f64 = balance_str.parse()?;
                    Ok((balance * token_metadata.denomination as f64) as Balance)
                } else {
                    let balance: f64 = input.parse()?;
                    Ok((balance * token_metadata.denomination as f64) as Balance)
                }
            }
        }
    }

    /// Display token units in a denominated format.
    pub fn from<T: Into<u128>>(value: T, token_metadata: Option<&TokenMetadata>) -> Self {
        let n: u128 = value.into();

        if let Some(token_metadata) = token_metadata {
            if n == 0 {
                return BalanceVariant::Denominated(format!("0{}", token_metadata.symbol))
            }

            let units_result = n / token_metadata.denomination;
            let mut symbol = "";
            let remainder: u128;
            let units: u128;
            if (1..1_000).contains(&units_result) {
                remainder = n % token_metadata.denomination;
                units = units_result;
            } else if (1_000..1_000_000).contains(&units_result) {
                remainder = units_result % 1_000;
                units = units_result / 1_000;
                symbol = "k";
            } else if (1_000_000..1_000_000_000).contains(&units_result) {
                remainder = units_result % 1_000_000;
                units = units_result / 1_000_000;
                symbol = "M";
            } else if n / 1_000_000_000 > 0 {
                remainder = n % 1_000_000_000;
                units = n / 1_000_000_000;
                symbol = "n";
            } else if n / 1_000_000 > 0 {
                remainder = n % 1_000_000;
                units = n / 1_000_000;
                symbol = "μ";
            } else {
                remainder = n % 1_000;
                units = n / 1_000;
                symbol = "m";
            }
            if remainder > 0 {
                let remainder = remainder.to_string().trim_end_matches('0').to_owned();
                BalanceVariant::Denominated(format!(
                    "{}.{}{}{}",
                    units, remainder, symbol, token_metadata.symbol
                ))
            } else {
                BalanceVariant::Denominated(format!(
                    "{}{}{}",
                    units, symbol, token_metadata.symbol
                ))
            }
        } else {
            BalanceVariant::Default(n)
        }
    }
}

impl Display for BalanceVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BalanceVariant::Default(balance) => f.write_str(&balance.to_string()),
            BalanceVariant::Denominated(input) => f.write_str(input),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cmd::extrinsics::parse_balance;

    use super::*;

    #[test]
    fn correct_balances_parses_success() {
        assert!(
            parse_balance("500DOT").is_ok(),
            "<500DOT> was not parsed correctly"
        );
        assert!(
            parse_balance("500").is_ok(),
            "<500> was not parsed correctly"
        );
    }

    #[test]
    fn incorrect_balances() {
        assert!(
            parse_balance("500%").is_err(),
            "expected to fail parsing incorrect balance"
        );
    }

    #[test]
    fn balance_variant_denominated_success() {
        let tm = TokenMetadata {
            denomination: 12,
            symbol: String::from("DOT"),
        };
        let bv = parse_balance("500MDOT").expect("successful parsing. qed");
        assert!(
            bv.denominate_balance(&tm).is_ok(),
            "balances could not be denominated correctly"
        );
    }

    #[test]
    fn balance_variant_denominated_incorrect_token() {
        let tm = TokenMetadata {
            denomination: 12,
            symbol: String::from("DOT"),
        };
        let bv = parse_balance("500MKSM").expect("successful parsing. qed");
        assert!(
            bv.denominate_balance(&tm).is_err(),
            "balances denominated should fail because of an incorrect token"
        );
    }

    #[test]
    fn balance_variant_denominated_equal() {
        let denomination: u128 = format!("1{}", "0".repeat(12)).parse().unwrap();
        let tm = TokenMetadata {
            denomination,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 500 * 1_000_000 * denomination;
        let bv = parse_balance("500MDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }

    #[test]
    fn balance_variant_denominated_equal_fraction() {
        let denomination: u128 = format!("1{}", "0".repeat(12)).parse().unwrap();
        let tm = TokenMetadata {
            denomination,
            symbol: String::from("DOT"),
        };
        let balance: Balance = (500 * 1_000_000 + 500_000) * denomination;
        let bv = parse_balance("500.5MDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }

    #[test]
    fn balance_variant_denominated_equal_small_units() {
        let denomination: u128 = format!("1{}", "0".repeat(12)).parse().unwrap();
        let tm = TokenMetadata {
            denomination,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 500 * 1_000_000 + 500_000;
        let bv = parse_balance("500.5μDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }
}

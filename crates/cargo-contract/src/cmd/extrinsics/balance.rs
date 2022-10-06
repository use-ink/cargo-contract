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

use std::{
    fmt::Display,
    str::FromStr,
};

use rust_decimal::{
    self,
    prelude::FromPrimitive,
    Decimal,
};
use serde_json::json;

use super::{
    Balance,
    Client,
};

use anyhow::{
    anyhow,
    Context,
    Ok,
    Result,
};

/// Represents different formats of a balance
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BalanceVariant {
    /// Default format: no symbol, no denomination
    Default(Balance),
    /// Denominated format: symbol and denomination are present
    Denominated(DenominatedBalance),
}

#[derive(Debug, Clone)]
pub struct TokenMetadata {
    /// Number of denomination used for denomination
    pub denomination: usize,
    /// Token symbol
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DenominatedBalance {
    value: Decimal,
    unit: UnitPrefix,
    symbol: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitPrefix {
    Giga,
    Mega,
    Kilo,
    One,
    Milli,
    Micro,
    Nano,
}

impl TokenMetadata {
    /// Query [TokenMetadata] through the node's RPC
    pub async fn query(client: &Client) -> Result<Self> {
        let sys_props = client.rpc().system_properties().await?;

        let default_decimals = json!(12);
        let default_units = json!("UNIT");
        let denomination = sys_props
            .get("tokenDecimal")
            .unwrap_or(&default_decimals)
            .as_u64()
            .context("error converting decimal to u64")?
            as usize;
        let symbol = sys_props
            .get("tokenSymbol")
            .unwrap_or(&default_units)
            .as_str()
            .context("error converting symbol to string")?;
        Ok(Self {
            denomination,
            symbol: symbol.to_string(),
        })
    }
}

impl FromStr for BalanceVariant {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.replace('_', "");
        if input.contains('.') || input.ends_with(|ch: char| ch.is_alphabetic()) {
            Ok(BalanceVariant::Denominated(DenominatedBalance::from_str(
                &input,
            )?))
        } else {
            Ok(BalanceVariant::Default(input.parse::<Balance>()?))
        }
    }
}

impl FromStr for DenominatedBalance {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let symbols = value
            .trim_start_matches(|ch: char| ch.is_numeric() || ch == '.' || ch == ',');
        let unit_char = symbols
            .chars()
            .next()
            .context("no units or symbols present")?;
        let unit: UnitPrefix = match unit_char {
            'G' => UnitPrefix::Giga,
            'M' => UnitPrefix::Mega,
            'K' => UnitPrefix::Kilo,
            'm' => UnitPrefix::Milli,
            '\u{3bc}' => UnitPrefix::Micro,
            'n' => UnitPrefix::Nano,
            _ => UnitPrefix::One,
        };
        let symbol = if unit != UnitPrefix::One {
            let (start, _) = symbols
                .char_indices()
                .nth(1)
                .context("cannot find the first char's index")?;
            symbols[start..].to_string()
        } else {
            String::new()
        };
        let value = value.trim_end_matches(|ch: char| ch.is_alphabetic());
        let value = Decimal::from_str_exact(value)
            .context("Error while parsing the value. Please denominate and normalize the balance first.")?
            .normalize();
        Ok(Self {
            value,
            unit,
            symbol,
        })
    }
}

impl BalanceVariant {
    /// Converts BalanceVariant into Balance.
    ///
    /// It is a reverse process of `from<T: Into<u128>>()`
    ///
    /// Throws Error if `value` is of nigher precision that allowed.
    ///
    /// ```rust
    ///  use anyhow::{Result, Ok};
    ///  let decimals = 6;
    ///  let tm = TokenMetadata {
    ///        denomination: decimals,
    ///        symbol: String::from("DOT"),
    /// };
    /// let sample_den_balance = Balance::Denominated(DenominatedBalance {
    ///     value: Decimal::new(4, 1),
    ///     unit: UnitPrefix::Micro,
    ///     symbol: String::new("DOT")
    /// });
    /// let result = sample_den_balance.denominate_balance(tm);
    /// assert!(result.is_err());
    /// ```
    ///
    /// Otherwise, [Balance] is returned:
    /// ```rust
    ///  use anyhow::{Result, Ok};
    ///  let decimals = 6;
    ///  let tm = TokenMetadata {
    ///        denomination: decimals,
    ///        symbol: String::from("DOT"),
    /// };
    /// let sample_den_balance = Balance::Denominated(DenominatedBalance {
    ///     value: Decimal::new(4123, 0),
    ///     unit: UnitPrefix::Micro,
    ///     symbol: String::new("DOT")
    /// });
    /// let balance = 4123;
    /// let result = sample_den_balance.denominate_balance(tm).unwrap()
    /// assert_eq!(balance, result);
    /// ```
    pub fn denominate_balance(&self, token_metadata: &TokenMetadata) -> Result<Balance> {
        match self {
            BalanceVariant::Default(balance) => Ok(*balance),
            BalanceVariant::Denominated(den_balance) => {
                let zeros: usize = (token_metadata.denomination as isize
                    + match den_balance.unit {
                        UnitPrefix::Giga => 9,
                        UnitPrefix::Mega => 6,
                        UnitPrefix::Kilo => 3,
                        UnitPrefix::One => 0,
                        UnitPrefix::Milli => -3,
                        UnitPrefix::Micro => -6,
                        UnitPrefix::Nano => -9,
                    })
                .try_into()?;
                let multiple =
                    Decimal::from_str_exact(&format!("1{}", "0".repeat(zeros)))?;
                let fract_scale = den_balance.value.fract().scale();
                let mantissa_difference = zeros as isize - fract_scale as isize;
                if mantissa_difference < 0 {
                    return Err(anyhow!(
                        "Given precision of a Balance value is higher than allowed"
                    ))
                }
                let balance: Balance = (den_balance.value * multiple).try_into()?;
                Ok(balance)
            }
        }
    }

    /// # Summary
    /// Display token units in a denominated format.
    ///
    /// I takes `value` of `Into<u128>` and [TokenMetadata]
    /// and calculates the value in an denominated format
    /// by manipulating the denomination.
    ///
    /// If the number is divisible by 10^(`denomination` + `unit_zeros`),
    /// It sets the [UnitPrefix] and divides the `value` into `Decimal`
    ///
    /// If no [TokenMetadata] was present, than that means
    /// that [Balance] is to be displayed in *normal* format
    /// and `BalanceVariant::Default` is returned
    ///
    /// # Examples
    /// ```rust
    ///  use anyhow::{Result, Ok};
    ///  let decimals = 10;
    ///  let tm = TokenMetadata {
    ///        denomination: decimals,
    ///        symbol: String::from("DOT"),
    /// };
    /// let sample_den_balance = BalanceVariant::Denominated(DenominatedBalance {
    ///     value: Decimal::new(5005, 1),
    ///     unit: UnitPrefix::Mega,
    ///     symbol: String::from("DOT")
    /// });
    /// let balance: Balance = 5_005_000_000_000_000_000;
    /// let den_balance = BalanceVariant::from(balance, Some(tm));
    /// assert_eq!(Ok(sample_den_balance), Ok(den_balance));
    /// ```
    pub fn from<T: Into<u128>>(
        value: T,
        token_metadata: Option<&TokenMetadata>,
    ) -> Result<Self> {
        let n: u128 = value.into();

        if let Some(token_metadata) = token_metadata {
            if n == 0 {
                return Ok(BalanceVariant::Denominated(DenominatedBalance {
                    value: Decimal::ZERO,
                    unit: UnitPrefix::One,
                    symbol: token_metadata.symbol.clone(),
                }))
            }

            let number_of_digits = n.to_string().len();

            let giga_units_zeros = token_metadata.denomination + 9;
            let mega_units_zeros = token_metadata.denomination + 6;
            let kilo_units_zeros = token_metadata.denomination + 3;
            let one_unit_zeros = token_metadata.denomination;
            let milli_units_zeros = token_metadata.denomination.checked_sub(3);
            let micro_units_zeros = token_metadata.denomination.checked_sub(6);
            let nano_units_zeros = token_metadata.denomination.checked_sub(9);

            let multiple: Decimal;
            let unit: UnitPrefix;
            if (giga_units_zeros + 1..).contains(&number_of_digits) {
                multiple = Decimal::from_str_exact(&format!(
                    "1{}",
                    "0".repeat(giga_units_zeros)
                ))?;
                unit = UnitPrefix::Giga;
            } else if (mega_units_zeros + 1..=giga_units_zeros)
                .contains(&number_of_digits)
            {
                multiple = Decimal::from_str_exact(&format!(
                    "1{}",
                    "0".repeat(mega_units_zeros)
                ))?;
                unit = UnitPrefix::Mega;
            } else if (kilo_units_zeros + 1..=mega_units_zeros)
                .contains(&number_of_digits)
            {
                multiple = Decimal::from_str_exact(&format!(
                    "1{}",
                    "0".repeat(kilo_units_zeros)
                ))?;
                unit = UnitPrefix::Kilo;
            } else if (one_unit_zeros + 1..=kilo_units_zeros).contains(&number_of_digits)
            {
                multiple =
                    Decimal::from_str_exact(&format!("1{}", "0".repeat(one_unit_zeros)))?;
                unit = UnitPrefix::One;
            } else if milli_units_zeros.is_some()
                && (milli_units_zeros.unwrap() + 1..=one_unit_zeros)
                    .contains(&number_of_digits)
            {
                multiple = Decimal::from_str_exact(&format!(
                    "1{}",
                    "0".repeat(milli_units_zeros.unwrap())
                ))?;
                unit = UnitPrefix::Milli;
            } else if milli_units_zeros.is_some()
                && micro_units_zeros.is_some()
                && (micro_units_zeros.unwrap() + 1..=milli_units_zeros.unwrap())
                    .contains(&number_of_digits)
            {
                multiple = Decimal::from_str_exact(&format!(
                    "1{}",
                    "0".repeat(micro_units_zeros.unwrap())
                ))?;
                unit = UnitPrefix::Micro;
            } else if nano_units_zeros.is_some() {
                multiple = Decimal::from_str_exact(&format!(
                    "1{}",
                    "0".repeat(nano_units_zeros.unwrap())
                ))?;
                unit = UnitPrefix::Nano;
            } else {
                return Err(anyhow!("Invalid denomination"))
            }
            let value = Decimal::from_u128(n)
                .context("value can not be converted into decimal")?
                / multiple;

            let den_balance = DenominatedBalance {
                value,
                unit,
                symbol: token_metadata.symbol.clone(),
            };

            Ok(BalanceVariant::Denominated(den_balance))
        } else {
            Ok(BalanceVariant::Default(n))
        }
    }
}

impl Display for BalanceVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BalanceVariant::Default(balance) => f.write_str(&balance.to_string()),
            BalanceVariant::Denominated(input) => f.write_str(&input.to_string()),
        }
    }
}

impl Display for DenominatedBalance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let prefix = match self.unit {
            UnitPrefix::Giga => "G",
            UnitPrefix::Mega => "M",
            UnitPrefix::Kilo => "K",
            UnitPrefix::One => "",
            UnitPrefix::Milli => "m",
            UnitPrefix::Micro => "μ",
            UnitPrefix::Nano => "n",
        };
        f.write_fmt(format_args!("{}{}{}", self.value, prefix, self.symbol))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correct_balances_parses_success() {
        assert!(
            BalanceVariant::from_str("500DOT").is_ok(),
            "<500DOT> was not parsed correctly"
        );
        assert!(
            BalanceVariant::from_str("500").is_ok(),
            "<500> was not parsed correctly"
        );
    }

    #[test]
    fn incorrect_balances() {
        assert!(
            BalanceVariant::from_str("500%").is_err(),
            "expected to fail parsing incorrect balance"
        );
    }

    #[test]
    fn balance_variant_denominated_success() {
        let tm = TokenMetadata {
            denomination: 10,
            symbol: String::from("DOT"),
        };
        let bv = BalanceVariant::from_str("500MDOT").expect("successful parsing. qed");
        assert!(
            bv.denominate_balance(&tm).is_ok(),
            "balances could not be denominated correctly"
        );
    }

    #[test]
    fn balance_variant_denominated_equal() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 500 * 1_000_000 * 10_000_000_000;
        let bv = BalanceVariant::from_str("500MDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }

    #[test]
    fn balance_variant_denominated_equal_fraction() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 5_005_000_000_000_000_000;
        let bv = BalanceVariant::from_str("500.5MDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }

    #[test]
    fn balance_variant_denominated_equal_small_units() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 5_005_000;
        let bv = BalanceVariant::from_str("500.5μDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }
    #[test]
    fn smallest_value() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 1;
        let bv = BalanceVariant::from_str("0.1nDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }

    #[test]
    fn value_less_than_precision() {
        // here we test if the user tries to input the denominated balance
        // which results in value less than zero
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let bv =
            BalanceVariant::from_str("0.01546nDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm);
        assert!(balance_parsed.is_err())
    }

    #[test]
    fn giga() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 5_005_000_000_000_000_000_000;
        let bv = BalanceVariant::from_str("500.5GDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }

    #[test]
    fn kilo() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 5_005_000_000_000_000;
        let bv = BalanceVariant::from_str("500.5KDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }

    #[test]
    fn unit() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 5_005_000_000_000;
        let bv = BalanceVariant::from_str("500.5DOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }

    #[test]
    fn milli() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 5_005_000_000;
        let bv = BalanceVariant::from_str("500.5mDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }
    #[test]
    fn micro() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 5_005_000;
        let bv = BalanceVariant::from_str("500.5μDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }
    #[test]
    fn nano() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 5_005;
        let bv = BalanceVariant::from_str("500.5nDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }

    #[test]
    fn different_digits() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 5_235_456_210_000_000;
        let bv =
            BalanceVariant::from_str("523.545621KDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }

    #[test]
    fn non_standard_denomination() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance: Balance = 50_015_000_000_000;
        let bv = BalanceVariant::from_str("5001.5DOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm).expect("successful parsing. qed");
        assert_eq!(balance, balance_parsed);
    }

    #[test]
    fn small_number_of_decimals_zero() {
        let decimals = 6;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let bv = BalanceVariant::from_str("0.4μDOT").expect("successful parsing. qed");
        let balance_parsed = bv.denominate_balance(&tm);
        assert!(balance_parsed.is_err())
    }

    #[test]
    fn big_input_to_denominate() {
        // max value of Decimal:MAX is 79_228_162_514_264_337_593_543_950_335
        let s = "79_228_162_514_264_337_593_543_950_336DOT";
        let bv = BalanceVariant::from_str(s);
        assert!(bv.is_err())
    }

    #[test]
    fn big_input_to_raw() {
        // max value of Decimal:MAX is 79_228_162_514_264_337_593_543_950_335
        let s = "79_228_162_514_264_337_593_543_950_336";
        let bv = BalanceVariant::from_str(s);
        assert!(bv.is_ok())
    }

    #[test]
    fn convert_from_u128() {
        let decimals = 6;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance = 532_500_000_000_u128;
        let denominated_balance =
            BalanceVariant::from(balance, Some(&tm)).expect("successful conversion");
        let sample = BalanceVariant::Denominated(DenominatedBalance {
            value: Decimal::new(5325, 1),
            unit: UnitPrefix::Kilo,
            symbol: String::from("DOT"),
        });
        assert_eq!(sample, denominated_balance);
    }

    #[test]
    fn convert_one_from_u128() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        let balance = 532_500_000_000_u128;
        let denominated_balance =
            BalanceVariant::from(balance, Some(&tm)).expect("successful conversion");
        let sample = BalanceVariant::Denominated(DenominatedBalance {
            value: Decimal::new(5325, 2),
            unit: UnitPrefix::One,
            symbol: String::from("DOT"),
        });
        assert_eq!(sample, denominated_balance);
    }

    #[test]
    fn convert_small_from_u128() {
        let decimals = 10;
        let tm = TokenMetadata {
            denomination: decimals,
            symbol: String::from("DOT"),
        };
        // 10_000_000_000 - One
        // 10_000_000 - Milli
        // 10_000 - Micro
        // 532_500 - 52.25 Micro
        let balance = 532_500_u128;
        let denominated_balance =
            BalanceVariant::from(balance, Some(&tm)).expect("successful conversion");
        let sample = BalanceVariant::Denominated(DenominatedBalance {
            value: Decimal::new(5325, 2),
            unit: UnitPrefix::Micro,
            symbol: String::from("DOT"),
        });
        assert_eq!(sample, denominated_balance);
    }
}

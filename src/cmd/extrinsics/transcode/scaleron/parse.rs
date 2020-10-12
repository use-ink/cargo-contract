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
// along with cargo-contract.  If not, see <http://www.gnu.org/license

use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take_while},
    character::complete::{alphanumeric1 as alphanumeric, char, one_of},
    combinator::{cut, map, opt, value},
    error::{context, convert_error, ErrorKind, ParseError, VerboseError},
    number::complete::double,
    sequence::{delimited, preceded, separated_pair, terminated},
    Err, IResult,
};
use super::RonValue;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum RonParseError {
    #[error("bad integer")]
    BadInt,
    #[error("bad float")]
    BadFloat,
    #[error("bad escape sequence")]
    BadEscape,
    #[error("unknown parser error")]
    Unparseable,
}

impl<I> ParseError<I> for RonParseError {
    fn from_error_kind(_input: I, _kind: ErrorKind) -> Self {
        RonParseError::Unparseable
    }

    fn append(_: I, _: ErrorKind, other: Self) -> Self {
        other
    }
}

fn ron_bool(input: &str) -> IResult<&str, RonValue, RonParseError> {
    alt((
        value(RonValue::Bool(false), tag("false")),
        value(RonValue::Bool(true), tag("true")),
    ))
        (input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_bool() {
        assert_eq!(ron_bool("false"), Ok(("", RonValue::Bool(false))));
        assert_eq!(ron_bool("true"), Ok(("", RonValue::Bool(true))));
        assert!(ron_bool("foo").is_err());
    }
}

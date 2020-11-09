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

use super::{Bytes, Map, Tuple, Value};
use escape8259::unescape;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::{alphanumeric1, anychar, char, digit0, hex_digit1, multispace0, one_of},
    combinator::{map, map_res, opt, recognize, value, verify},
    error::{ErrorKind, FromExternalError, ParseError},
    multi::{many0, many0_count, many1, separated_list0},
    sequence::{delimited, pair, preceded, separated_pair, tuple},
    IResult,
};
use std::{fmt::Debug, num::ParseIntError};

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum SonParseError {
    #[error("bad integer")]
    BadInt(#[from] ParseIntError),
    #[error("bad escape sequence")]
    BadEscape,
    #[error("hex string parse error")]
    BadHex(#[from] hex::FromHexError),
    #[error("parser error")]
    Nom(String, ErrorKind),
}

impl<I> FromExternalError<I, ParseIntError> for SonParseError {
    fn from_external_error(_input: I, _kind: ErrorKind, e: ParseIntError) -> Self {
        e.into()
    }
}

impl ParseError<&str> for SonParseError {
    fn from_error_kind(input: &str, kind: ErrorKind) -> Self {
        SonParseError::Nom(input.to_string(), kind)
    }

    fn append(_: &str, _: ErrorKind, other: Self) -> Self {
        other
    }
}

fn scon_string(input: &str) -> IResult<&str, Value, SonParseError> {
    // There are only two types of escape allowed by RFC 8259.
    // - single-character escapes \" \\ \/ \b \f \n \r \t
    // - general-purpose \uXXXX
    // Note: we don't enforce that escape codes are valid here.
    // There must be a decoder later on.
    fn escape_code(input: &str) -> IResult<&str, &str, SonParseError> {
        recognize(pair(
            tag("\\"),
            alt((
                tag("\""),
                tag("\\"),
                tag("/"),
                tag("b"),
                tag("f"),
                tag("n"),
                tag("r"),
                tag("t"),
                tag("u"),
            )),
        ))(input)
    }

    // Zero or more text characters
    fn string_body(input: &str) -> IResult<&str, &str, SonParseError> {
        recognize(many0(alt((nonescaped_string, escape_code))))(input)
    }

    fn string_literal(input: &str) -> IResult<&str, String, SonParseError> {
        let (remain, raw_string) = delimited(tag("\""), string_body, tag("\""))(input)?;

        match unescape(raw_string) {
            Ok(s) => Ok((remain, s)),
            Err(_) => Err(nom::Err::Failure(SonParseError::BadEscape)),
        }
    }

    map(string_literal, |s| Value::String(s))(input)
}

// A character that is:
// NOT a control character (0x00 - 0x1F)
// NOT a quote character (0x22)
// NOT a backslash character (0x5C)
// Is within the unicode range (< 0x10FFFF) (this is already guaranteed by Rust char)
fn is_nonescaped_string_char(c: char) -> bool {
    let cv = c as u32;
    (cv >= 0x20) && (cv != 0x22) && (cv != 0x5C)
}

// One or more unescaped text characters
fn nonescaped_string(input: &str) -> IResult<&str, &str, SonParseError> {
    take_while1(is_nonescaped_string_char)(input)
}

fn rust_ident(input: &str) -> IResult<&str, &str, SonParseError> {
    recognize(pair(
        verify(anychar, |&c| c.is_alphabetic() || c == '_'),
        many0_count(preceded(opt(char('_')), alphanumeric1)),
    ))(input)
}

fn digit1to9(input: &str) -> IResult<&str, char, SonParseError> {
    one_of("123456789")(input)
}

// unsigned_integer = zero / ( digit1-9 *DIGIT )
fn uint(input: &str) -> IResult<&str, &str, SonParseError> {
    alt((tag("0"), recognize(pair(digit1to9, digit0))))(input)
}

fn scon_integer(input: &str) -> IResult<&str, Value, SonParseError> {
    let signed = recognize(pair(char('-'), uint));

    alt((
        map_res(signed, |s| s.parse::<i128>().map(Value::Int)),
        map_res(uint, |s| s.parse::<u128>().map(Value::UInt)),
    ))(input)
}

fn scon_unit(input: &str) -> IResult<&str, Value, SonParseError> {
    let (i, _) = tag("()")(input)?;
    Ok((i, Value::Unit))
}

fn scon_bool(input: &str) -> IResult<&str, Value, SonParseError> {
    alt((
        value(Value::Bool(false), tag("false")),
        value(Value::Bool(true), tag("true")),
    ))(input)
}

fn scon_char(input: &str) -> IResult<&str, Value, SonParseError> {
    let parse_char = delimited(tag("'"), anychar, tag("'"));
    map(parse_char, |c| Value::Char(c))(input)
}

fn scon_seq(input: &str) -> IResult<&str, Value, SonParseError> {
    let opt_trailing_comma_close = pair(opt(ws(tag(","))), ws(tag("]")));

    let parser = delimited(
        ws(tag("[")),
        separated_list0(ws(tag(",")), scon_value),
        opt_trailing_comma_close,
    );
    map(parser, |v| Value::Seq(v.into()))(input)
}

fn scon_tuple(input: &str) -> IResult<&str, Value, SonParseError> {
    let opt_trailing_comma_close = pair(opt(ws(tag(","))), ws(tag(")")));
    let tuple_body = delimited(
        ws(tag("(")),
        separated_list0(ws(tag(",")), scon_value),
        opt_trailing_comma_close,
    );

    let parser = tuple((opt(ws(rust_ident)), tuple_body));

    map(parser, |(ident, v)| {
        Value::Tuple(Tuple::new(ident, v.into_iter().collect()))
    })(input)
}

/// Parse a rust ident on its own which could represent a struct with no fields or a enum unit
/// variant e.g. "None"
fn scon_unit_tuple(input: &str) -> IResult<&str, Value, SonParseError> {
    map(rust_ident, |ident| {
        Value::Tuple(Tuple::new(Some(ident), Vec::new()))
    })(input)
}

fn scon_map(input: &str) -> IResult<&str, Value, SonParseError> {
    let ident_key = map(rust_ident, |s| Value::String(s.into()));
    let scon_map_key = ws(alt((ident_key, scon_string, scon_integer)));

    let opening = alt((tag("("), tag("{")));
    let closing = alt((tag(")"), tag("}")));
    let entry = separated_pair(scon_map_key, ws(tag(":")), scon_value);

    let opt_trailing_comma_close = pair(opt(ws(tag(","))), ws(closing));
    let map_body = delimited(
        ws(opening),
        separated_list0(ws(tag(",")), entry),
        opt_trailing_comma_close,
    );

    let parser = tuple((opt(ws(rust_ident)), map_body));

    map(parser, |(ident, v)| {
        Value::Map(Map::new(ident, v.into_iter().collect()))
    })(input)
}

fn scon_bytes(input: &str) -> IResult<&str, Value, SonParseError> {
    let (rest, byte_str) = preceded(tag("0x"), hex_digit1)(input)?;
    let bytes = Bytes::from_hex_string(byte_str).map_err(|e| nom::Err::Failure(e.into()))?;
    Ok((rest, Value::Bytes(bytes)))
}

fn scon_literal(input: &str) -> IResult<&str, Value, SonParseError> {
    // let parser = recognize(ws(many1(alphanumeric1)));
    let parser = recognize(many1(alphanumeric1));
    map(parser, |literal: &str| Value::Literal(literal.into()))(input)
}

fn ws<F, I, O, E>(f: F) -> impl FnMut(I) -> IResult<I, O, E>
where
    F: FnMut(I) -> IResult<I, O, E>,
    I: nom::InputTakeAtPosition,
    <I as nom::InputTakeAtPosition>::Item: nom::AsChar + Clone,
    E: nom::error::ParseError<I>,
{
    delimited(multispace0, f, multispace0)
}

fn scon_value(input: &str) -> IResult<&str, Value, SonParseError> {
    ws(alt((
        scon_unit,
        scon_bytes,
        scon_seq,
        scon_tuple,
        scon_map,
        scon_string,
        scon_integer,
        scon_bool,
        scon_char,
        scon_unit_tuple,
        scon_literal,
    )))(input)
}

/// Attempt to parse a SON value
pub fn parse_value(input: &str) -> Result<Value, nom::Err<SonParseError>> {
    let (_, value) = scon_value(input)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_scon_value(input: &str, expected: Value) {
        assert_eq!(scon_value(input), Ok(("", expected)));
    }

    #[test]
    fn test_unit() {
        assert_eq!(scon_value("()"), Ok(("", Value::Unit)));
    }

    #[test]
    fn test_bool() {
        assert_eq!(scon_bool("false"), Ok(("", Value::Bool(false))));
        assert_eq!(scon_bool("true"), Ok(("", Value::Bool(true))));
        assert!(scon_bool("foo").is_err());
    }

    #[test]
    fn test_integer() {
        assert_eq!(scon_integer("42"), Ok(("", Value::UInt(42))));
        assert_eq!(scon_integer("-123"), Ok(("", Value::Int(-123))));
        assert_eq!(scon_integer("0"), Ok(("", Value::UInt(0))));
        assert_eq!(scon_integer("01"), Ok(("1", Value::UInt(0))));
        assert_eq!(
            scon_integer("340282366920938463463374607431768211455"),
            Ok(("", Value::UInt(340282366920938463463374607431768211455)))
        );
        // todo
        // assert!(matches!(scon_integer("abc123"), Err(nom::Err::Failure(RonParseError::BadInt(_)))));
        // // assert!(matches!(scon_integer("340282366920938463463374607431768211455"), Err(nom::Err::Failure(_))));
    }

    #[test]
    fn test_string() {
        // Plain Unicode strings with no escaping
        assert_eq!(scon_string(r#""""#), Ok(("", Value::String("".into()))));
        assert_eq!(
            scon_string(r#""Hello""#),
            Ok(("", Value::String("Hello".into())))
        );
        assert_eq!(scon_string(r#""の""#), Ok(("", Value::String("の".into()))));
        assert_eq!(scon_string(r#""𝄞""#), Ok(("", Value::String("𝄞".into()))));

        // valid 2-character escapes
        assert_eq!(
            scon_string(r#""  \\  ""#),
            Ok(("", Value::String("  \\  ".into())))
        );
        assert_eq!(
            scon_string(r#""  \"  ""#),
            Ok(("", Value::String("  \"  ".into())))
        );

        // valid 6-character escapes
        assert_eq!(
            scon_string(r#""\u0000""#),
            Ok(("", Value::String("\x00".into())))
        );
        assert_eq!(
            scon_string(r#""\u00DF""#),
            Ok(("", Value::String("ß".into())))
        );
        assert_eq!(
            scon_string(r#""\uD834\uDD1E""#),
            Ok(("", Value::String("𝄞".into())))
        );

        // Invalid because surrogate characters must come in pairs
        assert!(scon_string(r#""\ud800""#).is_err());
        // Unknown 2-character escape
        assert!(scon_string(r#""\x""#).is_err());
        // Not enough hex digits
        assert!(scon_string(r#""\u""#).is_err());
        assert!(scon_string(r#""\u001""#).is_err());
        // Naked control character
        assert!(scon_string(r#""\x0a""#).is_err());
        // Not a JSON string because it's not wrapped in quotes
        assert!(scon_string("abc").is_err());
        // An unterminated string (because the trailing quote is escaped)
        assert!(scon_string(r#""\""#).is_err());

        // Parses correctly but has escape errors due to incomplete surrogate pair.
        assert_eq!(
            scon_string(r#""\ud800""#),
            Err(nom::Err::Failure(SonParseError::BadEscape))
        );
    }

    #[test]
    fn test_seq() {
        assert_eq!(scon_value("[ ]"), Ok(("", Value::Seq(vec![].into()))));
        assert_eq!(
            scon_value("[ 1 ]"),
            Ok(("", Value::Seq(vec![Value::UInt(1)].into())))
        );

        let expected = Value::Seq(vec![Value::UInt(1), Value::String("x".into())].into());
        assert_eq!(scon_value(r#" [ 1 , "x" ] "#), Ok(("", expected)));

        let trailing = r#"["a", "b",]"#;
        assert_eq!(
            scon_value(trailing),
            Ok((
                "",
                Value::Seq(vec![Value::String("a".into()), Value::String("b".into())].into())
            ))
        );
    }

    #[test]
    fn test_rust_ident() {
        assert_eq!(rust_ident("a"), Ok(("", "a")));
        assert_eq!(rust_ident("a:"), Ok((":", "a")));
        assert_eq!(rust_ident("Ok"), Ok(("", "Ok")));
        assert_eq!(rust_ident("_ok"), Ok(("", "_ok")));
        assert_eq!(rust_ident("im_ok"), Ok(("", "im_ok")));
        // assert_eq!(rust_ident("im_ok_"), Ok(("", "im_ok_"))); // todo
        assert_eq!(rust_ident("im_ok_123abc"), Ok(("", "im_ok_123abc")));
        assert!(rust_ident("1notok").is_err());
    }

    #[test]
    fn test_literal() {
        assert_scon_value(
            "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
            Value::Literal("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY".into()),
        );
    }

    #[test]
    fn test_map() {
        assert_eq!(
            scon_value("Foo {}"),
            Ok(("", Value::Map(Map::new(Some("Foo"), Default::default()))))
        );
        assert_eq!(
            scon_value("Foo{}"),
            Ok(("", Value::Map(Map::new(Some("Foo"), Default::default()))))
        );

        assert_eq!(rust_ident("a:"), Ok((":", "a")));

        assert_eq!(
            scon_value(r#"(a: 1)"#),
            Ok((
                "",
                Value::Map(Map::new(
                    None,
                    vec![(Value::String("a".into()), Value::UInt(1)),]
                        .into_iter()
                        .collect()
                ))
            ))
        );

        assert_eq!(
            scon_value(r#"A (a: 1, b: "bar")"#),
            Ok((
                "",
                Value::Map(Map::new(
                    Some("A"),
                    vec![
                        (Value::String("a".into()), Value::UInt(1)),
                        (Value::String("b".into()), Value::String("bar".into())),
                    ]
                    .into_iter()
                    .collect()
                ))
            ))
        );

        assert_eq!(
            scon_value(r#"B(a: 1)"#),
            Ok((
                "",
                Value::Map(Map::new(
                    Some("B"),
                    vec![(Value::String("a".into()), Value::UInt(1)),]
                        .into_iter()
                        .collect()
                ))
            ))
        );

        assert_eq!(
            scon_value(r#"Struct { a : 1 }"#),
            Ok((
                "",
                Value::Map(Map::new(
                    Some("Struct"),
                    vec![(Value::String("a".into()), Value::UInt(1)),]
                        .into_iter()
                        .collect()
                ))
            ))
        );

        let map = r#"Mixed {
            1: "a",
            "b": 2,
            c: true,
        }"#;

        assert_eq!(
            scon_value(map),
            Ok((
                "",
                Value::Map(Map::new(
                    Some("Struct"),
                    vec![
                        (Value::UInt(1), Value::String("a".into())),
                        (Value::String("b".into()), Value::UInt(2)),
                        (Value::String("c".into()), Value::Bool(true)),
                        // (Value::String("d".into()), Value::Literal("5ALiteral".into())),
                    ]
                    .into_iter()
                    .collect()
                ))
            ))
        );
    }

    #[test]
    fn test_tuple() {
        assert_scon_value("Foo ()", Value::Tuple(Tuple::new(Some("Foo"), vec![])));
        assert_scon_value("Foo()", Value::Tuple(Tuple::new(Some("Foo"), vec![])));
        assert_scon_value("Foo", Value::Tuple(Tuple::new(Some("Foo"), vec![])));

        assert_scon_value(
            r#"B("a")"#,
            Value::Tuple(Tuple::new(Some("B"), vec![Value::String("a".into())])),
        );
        assert_scon_value(
            r#"B("a", 10, true)"#,
            Value::Tuple(Tuple::new(
                Some("B"),
                vec![
                    Value::String("a".into()),
                    Value::UInt(10),
                    Value::Bool(true),
                ],
            )),
        );

        assert_scon_value(
            r#"Mixed ("a", 10, ["a", "b", "c"],)"#,
            Value::Tuple(Tuple::new(
                Some("Mixed"),
                vec![
                    Value::String("a".into()),
                    Value::UInt(10),
                    Value::Seq(
                        vec![
                            Value::String("a".into()),
                            Value::String("b".into()),
                            Value::String("c".into()),
                        ]
                        .into(),
                    ),
                ],
            )),
        );

        assert_scon_value(
            r#"(Nested("a", 10))"#,
            Value::Tuple(Tuple::new(
                None,
                vec![Value::Tuple(Tuple::new(
                    Some("Nested"),
                    vec![Value::String("a".into()), Value::UInt(10)],
                ))],
            )),
        )
    }

    #[test]
    fn test_option() {
        assert_scon_value(
            r#"Some("a")"#,
            Value::Tuple(Tuple::new(Some("Some"), vec![Value::String("a".into())])),
        );
        assert_scon_value(
            r#"None"#,
            Value::Tuple(Tuple::new(Some("None"), Vec::new())),
        );
    }

    #[test]
    fn test_char() {
        assert_scon_value(r#"'c'"#, Value::Char('c'));
    }

    #[test]
    fn test_bytes() {
        assert_scon_value(r#"0x0000"#, Value::Bytes(vec![0u8; 2].into()));
    }
}

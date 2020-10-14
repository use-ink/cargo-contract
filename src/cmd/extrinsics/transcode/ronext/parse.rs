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
    bytes::complete::{tag, take_while1},
    character::complete::{anychar, alphanumeric1, one_of, digit0, multispace0, char},
    combinator::{map, opt, recognize, value, verify},
    error::{ErrorKind, ParseError},
    multi::{many0, many0_count, separated_list},
    sequence::{delimited, pair, separated_pair, tuple, preceded},
    IResult,
};
use escape8259::unescape;
use std::num::ParseIntError;
use super::{
    Bytes,
    Map,
    Value,
    Tuple,
};
use nom::combinator::map_res;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum RonParseError {
    #[error("bad integer")]
    BadInt(#[from] ParseIntError),
    #[error("bad escape sequence")]
    BadEscape,
    #[error("hex string parse error")]
    BadHex(#[from] hex::FromHexError),
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

fn ron_string(input: &str) -> IResult<&str, Value, RonParseError> {
    // There are only two types of escape allowed by RFC 8259.
    // - single-character escapes \" \\ \/ \b \f \n \r \t
    // - general-purpose \uXXXX
    // Note: we don't enforce that escape codes are valid here.
    // There must be a decoder later on.
    fn escape_code(input: &str) -> IResult<&str, &str, RonParseError> {
        recognize(
            pair(
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
                ))
            )
        )(input)
    }

    // Zero or more text characters
    fn string_body(input: &str) -> IResult<&str, &str, RonParseError> {
        recognize(
            many0(
                alt((
                    nonescaped_string,
                    escape_code
                ))
            )
        )(input)
    }

    fn string_literal(input: &str) -> IResult<&str, String, RonParseError> {
        let (remain, raw_string) = delimited(
            tag("\""),
            string_body,
            tag("\"")
        )(input)?;

        match unescape(raw_string) {
            Ok(s) => Ok((remain, s)),
            Err(_) => Err(nom::Err::Failure(RonParseError::BadEscape)),
        }
    }

    map(string_literal, |s| {
        Value::String(s)
    })(input)
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
fn nonescaped_string(input: &str) -> IResult<&str, &str, RonParseError> {
    take_while1(is_nonescaped_string_char)
        (input)
}

fn rust_ident(input: &str) -> IResult<&str, &str, RonParseError> {
    recognize(pair(
        verify(anychar, |&c| c.is_alphabetic() || c == '_'),
        many0_count(preceded(opt(char('_')), alphanumeric1))
    ))(input)
}

fn digit1to9(input: &str) -> IResult<&str, char, RonParseError> {
    one_of("123456789")
        (input)
}

// unsigned_integer = zero / ( digit1-9 *DIGIT )
fn uint(input: &str) -> IResult<&str, &str, RonParseError> {
    alt((
        tag("0"),
        recognize(
            pair(
                digit1to9,
                digit0
            )
        )
    ))
        (input)
}

fn ron_integer(input: &str) -> IResult<&str, Value, RonParseError> {
    let signed = recognize(pair(
        char('-'),
        uint
    ));

    alt((
        map_res(signed, |s| s.parse::<i128>().map_err(RonParseError::BadInt).map(Value::Int)),
        map_res(uint, |s| s.parse::<u128>().map_err(RonParseError::BadInt).map(Value::UInt))
        ))(input)
}

fn ron_unit(input: &str) -> IResult<&str, Value, RonParseError> {
    let (i, _) = tag("()")(input)?;
    Ok((i, Value::Unit))
}

fn ron_bool(input: &str) -> IResult<&str, Value, RonParseError> {
    alt((
        value(Value::Bool(false), tag("false")),
        value(Value::Bool(true), tag("true")),
    ))(input)
}

fn ron_char(input: &str) -> IResult<&str, Value, RonParseError> {
    let parse_char = delimited(tag("'"), anychar, tag("'"));
    map(parse_char, |c| Value::Char(c))(input)
}

fn ron_seq(input: &str) -> IResult<&str, Value, RonParseError> {
    let opt_trailing_comma_close = pair(opt(ws(tag(","))), ws(tag("]")));

    let parser = delimited(
        ws(tag("[")),
        separated_list(ws(tag(",")), ron_value),
        opt_trailing_comma_close,
    );
    map(parser, |v| {
        Value::Seq(v.into())
    })
        (input)
}

fn ron_option(input: &str) -> IResult<&str, Value, RonParseError> {
    let none = value(Value::Option(None), tag("None"));
    let some_value = map(ron_value, |v| Value::Option(Some(v.into())));
    let some = preceded(
        tag("Some"),
        delimited(
            ws(tag("(")),
            some_value,
             ws(tag(")"))
    ));
    alt((none, some))(input)
}

fn ron_tuple(input: &str) -> IResult<&str, Value, RonParseError> {
    let opt_trailing_comma_close = pair(opt(ws(tag(","))), ws(tag(")")));
    let tuple_body = delimited(
        ws(tag("(")),
        separated_list(ws(tag(",")), ron_value),
        opt_trailing_comma_close,
    );

    let parser = tuple((opt(ws(rust_ident)), tuple_body));

    map(parser, |(ident, v)| {
        Value::Tuple(Tuple::new(ident, v.into_iter().collect()))
    })(input)
}

fn ron_map(input: &str) -> IResult<&str, Value, RonParseError> {
    let ident_key = map(rust_ident, |s| Value::String(s.into()));
    let ron_map_key = ws(alt((
        ident_key,
        ron_string,
        ron_integer,
    )));

    let opening = alt((tag("("), tag("{")));
    let closing = alt((tag(")"), tag("}")));
    let entry = separated_pair(ron_map_key, ws(tag(":")), ron_value);

    let opt_trailing_comma_close = pair(opt(ws(tag(","))), ws(closing));
    let map_body = delimited(
        ws(opening),
        separated_list(ws(tag(",")), entry),
        opt_trailing_comma_close,
    );

    let parser = tuple((opt(ws(rust_ident)), map_body));

    map(parser, |(ident, v)| {
        Value::Map(Map::new(ident, v.into_iter().collect()))
    })(input)
}

fn ron_bytes(input: &str) -> IResult<&str, Value, RonParseError> {
    let (rest, byte_str) = preceded(tag("0x"), nom::character::complete::hex_digit1)(input)?;
    let bytes = Bytes::from_hex_string(byte_str)
        .map_err(|e| nom::Err::Failure(e.into()))?;
    Ok((rest, Value::Bytes(bytes)))
}

fn ws<F, I, O, E>(f: F) -> impl Fn(I) -> IResult<I, O, E>
    where
        F: Fn(I) -> IResult<I, O, E>,
        I: nom::InputTakeAtPosition,
        <I as nom::InputTakeAtPosition>::Item: nom::AsChar + Clone,
        E: nom::error::ParseError<I>,
{
    delimited(multispace0, f, multispace0)
}

fn ron_value(input: &str) -> IResult<&str, Value, RonParseError> {
    ws(alt((
        ron_unit,
        ron_bytes,
        ron_option,
        ron_seq,
        ron_tuple,
        ron_map,
        ron_string,
        ron_integer,
        ron_bool,
        ron_char,
    )))
        (input)
}

/// Attempt to parse a RON (extended) value
pub fn parse_value(input: &str) -> Result<Value, nom::Err<RonParseError>> {
    let (_, value) = ron_value(input)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_ron_value(input: &str, expected: Value) {
        assert_eq!(ron_value(input), Ok(("", expected)));
    }

    #[test]
    fn test_unit() {
        assert_eq!(ron_value("()"), Ok(("", Value::Unit)));
    }

    #[test]
    fn test_bool() {
        assert_eq!(ron_bool("false"), Ok(("", Value::Bool(false))));
        assert_eq!(ron_bool("true"), Ok(("", Value::Bool(true))));
        assert!(ron_bool("foo").is_err());
    }

    #[test]
    fn test_integer() {
        assert_eq!(ron_integer("42"), Ok(("", Value::UInt(42))));
        assert_eq!(ron_integer("-123"), Ok(("", Value::Int(-123))));
        assert_eq!(ron_integer("0"), Ok(("", Value::UInt(0))));
        assert_eq!(ron_integer("01"), Ok(("1", Value::UInt(0))));
        assert_eq!(ron_integer("340282366920938463463374607431768211455"), Ok(("", Value::UInt(340282366920938463463374607431768211455))));
        // todo
        // assert!(matches!(ron_integer("abc123"), Err(nom::Err::Failure(RonParseError::BadInt(_)))));
        // // assert!(matches!(ron_integer("340282366920938463463374607431768211455"), Err(nom::Err::Failure(_))));
    }

    #[test]
    fn test_string() {
        // Plain Unicode strings with no escaping
        assert_eq!(ron_string(r#""""#), Ok(("", Value::String("".into()))));
        assert_eq!(ron_string(r#""Hello""#), Ok(("", Value::String("Hello".into()))));
        assert_eq!(ron_string(r#""の""#), Ok(("", Value::String("の".into()))));
        assert_eq!(ron_string(r#""𝄞""#), Ok(("", Value::String("𝄞".into()))));

        // valid 2-character escapes
        assert_eq!(ron_string(r#""  \\  ""#), Ok(("", Value::String("  \\  ".into()))));
        assert_eq!(ron_string(r#""  \"  ""#), Ok(("", Value::String("  \"  ".into()))));

        // valid 6-character escapes
        assert_eq!(ron_string(r#""\u0000""#), Ok(("", Value::String("\x00".into()))));
        assert_eq!(ron_string(r#""\u00DF""#), Ok(("", Value::String("ß".into()))));
        assert_eq!(ron_string(r#""\uD834\uDD1E""#), Ok(("", Value::String("𝄞".into()))));

        // Invalid because surrogate characters must come in pairs
        assert!(ron_string(r#""\ud800""#).is_err());
        // Unknown 2-character escape
        assert!(ron_string(r#""\x""#).is_err());
        // Not enough hex digits
        assert!(ron_string(r#""\u""#).is_err());
        assert!(ron_string(r#""\u001""#).is_err());
        // Naked control character
        assert!(ron_string(r#""\x0a""#).is_err());
        // Not a JSON string because it's not wrapped in quotes
        assert!(ron_string("abc").is_err());
        // An unterminated string (because the trailing quote is escaped)
        assert!(ron_string(r#""\""#).is_err());

        // Parses correctly but has escape errors due to incomplete surrogate pair.
        assert_eq!(ron_string(r#""\ud800""#), Err(nom::Err::Failure(RonParseError::BadEscape)));
    }

    #[test]
    fn test_seq() {
        assert_eq!(ron_value("[ ]"), Ok(("", Value::Seq(vec![].into()))));
        assert_eq!(ron_value("[ 1 ]"), Ok(("", Value::Seq(vec![Value::UInt(1)].into()))));

        let expected = Value::Seq(vec![Value::UInt(1), Value::String("x".into())].into());
        assert_eq!(ron_value(r#" [ 1 , "x" ] "#), Ok(("", expected)));

        let trailing = r#"["a", "b",]"#;
        assert_eq!(ron_value(trailing), Ok(("", Value::Seq(vec![Value::String("a".into()), Value::String("b".into())]))));
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
    fn test_map() {
        assert_eq!(ron_value("Foo {}"), Ok(("", Value::Map(Map::new(Some("Foo"), Default::default())))));
        assert_eq!(ron_value("Foo{}"), Ok(("", Value::Map(Map::new(Some("Foo"), Default::default())))));

        assert_eq!(rust_ident("a:"), Ok((":", "a")));

        assert_eq!(ron_value(r#"(a: 1)"#), Ok(("", Value::Map(Map::new(None, vec![
            (Value::String("a".into()), Value::UInt(1)),
        ].into_iter().collect())))));

        assert_eq!(ron_value(r#"A (a: 1, b: "bar")"#), Ok(("", Value::Map(Map::new(Some("A"), vec![
            (Value::String("a".into()), Value::UInt(1)),
            (Value::String("b".into()), Value::String("bar".into())),
        ].into_iter().collect())))));

        assert_eq!(ron_value(r#"B(a: 1)"#), Ok(("", Value::Map(Map::new(Some("B"), vec![
            (Value::String("a".into()), Value::UInt(1)),
        ].into_iter().collect())))));

        assert_eq!(ron_value(r#"Struct { a : 1 }"#), Ok(("", Value::Map(Map::new(Some("Struct"), vec![
            (Value::String("a".into()), Value::UInt(1)),
        ].into_iter().collect())))));

        let map = r#"Mixed {
            1: "a",
            "b": 2,
            c: true,
        }"#;

        assert_eq!(ron_value(map), Ok(("", Value::Map(Map::new(Some("Struct"), vec![
            (Value::UInt(1), Value::String("a".into())),
            (Value::String("b".into()), Value::UInt(2)),
            (Value::String("c".into()), Value::Bool(true)),
        ].into_iter().collect())))));
    }

    #[test]
    fn test_tuple() {
        assert_eq!(ron_value("Foo ()"), Ok(("", Value::Tuple(Tuple::new(Some("Foo"), Default::default())))));
        assert_eq!(ron_value("Foo()"), Ok(("", Value::Tuple(Tuple::new(Some("Foo"), Default::default())))));

        assert_eq!(ron_value(r#"B("a")"#), Ok(("", Value::Tuple(Tuple::new(Some("B"), vec![
            Value::String("a".into()),
        ])))));

        assert_eq!(ron_value(r#"B("a", 10, true)"#), Ok(("", Value::Tuple(Tuple::new(Some("B"), vec![
            Value::String("a".into()),
            Value::UInt(10),
            Value::Bool(true),
        ])))));

        let tuple = r#"Mixed ("a", 10, ["a", "b", "c"],)"#;

        assert_eq!(ron_value(tuple), Ok(("", Value::Tuple(Tuple::new(Some("Mixed"), vec![
            Value::String("a".into()),
            Value::UInt(10),
            Value::Seq(vec![Value::String("a".into()), Value::String("b".into()), Value::String("c".into())]),
        ])))));

        let nested = r#"(Nested("a", 10))"#;

        let expected = Value::Tuple(Tuple::new(None, vec![
            Value::Tuple(Tuple::new(Some("Nested"), vec![
                Value::String("a".into()),
                Value::UInt(10),
            ]))
        ]));

        assert_eq!(ron_value(nested), Ok(("", expected)));
    }

    #[test]
    fn test_option() {
        assert_ron_value(r#"Some("a")"#, Value::Option(Some(Value::String("a".into()).into())));
        assert_ron_value(r#"None"#, Value::Option(None));
    }

    #[test]
    fn test_char() {
        assert_ron_value(r#"'c'"#, Value::Char('c'));
    }

    #[test]
    fn test_bytes() {
        assert_ron_value(r#"0x0000"#, Value::Bytes(vec![0u8; 2].into()));
    }
}

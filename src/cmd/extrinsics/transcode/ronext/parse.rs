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
    bytes::complete::{escaped, tag, take_while, take_while1},
    character::complete::{alphanumeric0, alpha0, alpha1, anychar, alphanumeric1, one_of, digit0, digit1, multispace0, char},
    combinator::{all_consuming, map, opt, recognize, value, verify},
    error::{context, convert_error, ErrorKind, ParseError, VerboseError},
    multi::{many0, many0_count, separated_list},
    number::complete::double,
    sequence::{delimited, pair, separated_pair, tuple, preceded},
    Err, IResult,
};
use escape8259::unescape;
use super::{
    RonMap,
    RonValue,
};

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

fn ron_string(input: &str) -> IResult<&str, RonValue, RonParseError> {
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
        RonValue::String(s)
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

fn integer_body(input: &str) -> IResult<&str, &str, RonParseError> {
    recognize(
        pair(
            opt(tag("-")),
            uint
        )
    )
        (input)
}

fn ron_integer(input: &str) -> IResult<&str, RonValue, RonParseError> {
    let (remain, raw_int) = integer_body(input)?;
    match raw_int.parse::<i64>() {
        Ok(i) => Ok((remain, RonValue::Number(ron::Number::Integer(i)))),
        Err(_) => Err(nom::Err::Failure(RonParseError::BadInt)),
    }
}

fn ron_bool(input: &str) -> IResult<&str, RonValue, RonParseError> {
    alt((
        value(RonValue::Bool(false), tag("false")),
        value(RonValue::Bool(true), tag("true")),
    ))(input)
}

fn ron_seq(input: &str) -> IResult<&str, RonValue, RonParseError> {
    let parser = delimited(
        ws(tag("[")),
        separated_list(ws(tag(",")), ron_value),
        ws(tag("]")),
    );
    map(parser, |v| {
        RonValue::Seq(v.into())
    })
        (input)
}

fn ron_map(input: &str) -> IResult<&str, RonValue, RonParseError> {
    let ident_key = map(rust_ident, |s| RonValue::String(s.into()));
    // let ron_map_key = ws(alt((
    //     ident_key,
    //     ron_string,
    //     ron_integer,
    // )));

    let opening = alt((tag("("), tag("{")));
    let closing = alt((tag(")"), tag("}")));
    let entry = separated_pair(ws(ident_key), tag(":"), ron_value);

    let map_body = delimited(
        ws(opening),
        separated_list(ws(tag(",")), entry),
        ws(closing),
    );

    let parser = tuple((opt(ws(rust_ident)), map_body));

    map(parser, |(ident, v)| {
        RonValue::Map(RonMap::new(ident, v.into_iter().collect()))
    })(input)
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

fn ron_value(input: &str) -> IResult<&str, RonValue, RonParseError> {
    ws(alt((
        ron_seq,
        ron_map,
        ron_string,
        // json_float,
        ron_integer,
        ron_bool,
    )))
        (input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bool() {
        assert_eq!(ron_bool("false"), Ok(("", RonValue::Bool(false))));
        assert_eq!(ron_bool("true"), Ok(("", RonValue::Bool(true))));
        assert!(ron_bool("foo").is_err());
    }

    #[test]
    fn test_integer() {
        assert_eq!(ron_integer("42"), Ok(("", RonValue::Number(ron::Number::Integer(42)))));
        assert_eq!(ron_integer("-123"), Ok(("", RonValue::Number(ron::Number::Integer(-123)))));
        assert_eq!(ron_integer("0"), Ok(("", RonValue::Number(ron::Number::Integer(0)))));
        assert_eq!(ron_integer("01"), Ok(("1", RonValue::Number(ron::Number::Integer(0)))));
        assert_eq!(ron_integer("9999999999999999999"), Err(nom::Err::Failure(RonParseError::BadInt)));
    }

    #[test]
    fn test_string() {
        // Plain Unicode strings with no escaping
        assert_eq!(ron_string(r#""""#), Ok(("", RonValue::String("".into()))));
        assert_eq!(ron_string(r#""Hello""#), Ok(("", RonValue::String("Hello".into()))));
        assert_eq!(ron_string(r#""の""#), Ok(("", RonValue::String("の".into()))));
        assert_eq!(ron_string(r#""𝄞""#), Ok(("", RonValue::String("𝄞".into()))));

        // valid 2-character escapes
        assert_eq!(ron_string(r#""  \\  ""#), Ok(("", RonValue::String("  \\  ".into()))));
        assert_eq!(ron_string(r#""  \"  ""#), Ok(("", RonValue::String("  \"  ".into()))));

        // valid 6-character escapes
        assert_eq!(ron_string(r#""\u0000""#), Ok(("", RonValue::String("\x00".into()))));
        assert_eq!(ron_string(r#""\u00DF""#), Ok(("", RonValue::String("ß".into()))));
        assert_eq!(ron_string(r#""\uD834\uDD1E""#), Ok(("", RonValue::String("𝄞".into()))));

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
        assert_eq!(ron_seq("[ ]"), Ok(("", RonValue::Seq(vec![].into()))));
        assert_eq!(ron_seq("[ 1 ]"), Ok(("", RonValue::Seq(vec![RonValue::Number(ron::Number::Integer(1))].into()))));

        let expected = RonValue::Seq(vec![RonValue::Number(ron::Number::Integer(1)), RonValue::String("x".into())].into());
        assert_eq!(ron_seq(r#" [ 1 , "x" ] "#), Ok(("", expected)));
    }

    #[test]
    fn test_rust_ident() {
        assert_eq!(rust_ident("a"), Ok(("", "a")));
        assert_eq!(rust_ident("a:"), Ok((":", "a")));
        assert_eq!(rust_ident("Ok"), Ok(("", "Ok")));
        assert_eq!(rust_ident("_ok"), Ok(("", "_ok")));
        assert!(rust_ident("1notok").is_err());
    }

    #[test]
    fn test_map() {
        // assert_eq!(ron_value("()"), Ok(("", RonValue::Map(RonMap::new(None, Default::default())))));
        // assert_eq!(ron_value("{}"), Ok(("", RonValue::Map(RonMap::new(None, Default::default())))));
        //
        // assert_eq!(ron_value("Foo ()"), Ok(("", RonValue::Map(RonMap::new(Some("Foo"), Default::default())))));
        // assert_eq!(ron_value("Foo()"), Ok(("", RonValue::Map(RonMap::new(Some("Foo"), Default::default())))));
        // assert_eq!(ron_value("Foo {}"), Ok(("", RonValue::Map(RonMap::new(Some("Foo"), Default::default())))));
        // assert_eq!(ron_value("Foo{}"), Ok(("", RonValue::Map(RonMap::new(Some("Foo"), Default::default())))));

        assert_eq!(rust_ident("a:"), Ok((":", "a")));

        assert_eq!(ron_value(r#"(a: 1)"#), Ok(("", RonValue::Map(RonMap::new(None, vec![
            (RonValue::String("a".into()), RonValue::Number(ron::Number::Integer(1))),
        ].into_iter().collect())))));

        assert_eq!(ron_value(r#"A (a: 1, b: "bar")"#), Ok(("", RonValue::Map(RonMap::new(Some("A"), vec![
            (RonValue::String("a".into()), RonValue::Number(ron::Number::Integer(1))),
            (RonValue::String("b".into()), RonValue::String("bar".into())),
        ].into_iter().collect())))));

        assert_eq!(ron_value(r#"B(a: 1)"#), Ok(("", RonValue::Map(RonMap::new(Some("B"), vec![
            (RonValue::String("a".into()), RonValue::Number(ron::Number::Integer(1))),
        ].into_iter().collect())))));

        assert_eq!(ron_value(r#"B { a: 1 }"#), Ok(("", RonValue::Map(RonMap::new(Some("B"), vec![
            (RonValue::String("a".into()), RonValue::Number(ron::Number::Integer(1))),
        ].into_iter().collect())))));
    }
}

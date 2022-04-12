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

use super::{
    Bytes,
    Map,
    Tuple,
    Value,
};
use escape8259::unescape;
use nom::{
    branch::alt,
    bytes::complete::{
        tag,
        take_while1,
    },
    character::complete::{
        alphanumeric1,
        anychar,
        char,
        digit1,
        hex_digit1,
        multispace0,
    },
    multi::{
        many0,
        separated_list0,
    },
    sequence::{
        delimited,
        pair,
        separated_pair,
        tuple,
    },
    AsChar,
    IResult,
    Parser,
};
use nom_supreme::{
    error::ErrorTree,
    ParserExt,
};

/// Attempt to parse a SCON value
pub fn parse_value(input: &str) -> anyhow::Result<Value> {
    let (_, value) = scon_value(input)
        .map_err(|err| anyhow::anyhow!("Error parsing Value: {}", err))?;
    Ok(value)
}

fn scon_value(input: &str) -> IResult<&str, Value, ErrorTree<&str>> {
    ws(alt((
        scon_unit,
        scon_bytes,
        scon_seq,
        scon_tuple,
        scon_map,
        scon_string,
        scon_literal,
        scon_integer,
        scon_bool,
        scon_char,
        scon_unit_tuple,
    )))
    .context("Value")
    .parse(input)
}

fn scon_string(input: &str) -> IResult<&str, Value, ErrorTree<&str>> {
    #[derive(Debug)]
    struct UnescapeError(String);
    impl std::error::Error for UnescapeError {}

    impl std::fmt::Display for UnescapeError {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "Error unescaping string '{}'", self.0)
        }
    }

    // One or more unescaped text characters
    let nonescaped_string = take_while1(|c| {
        let cv = c as u32;
        // A character that is:
        // NOT a control character (0x00 - 0x1F)
        // NOT a quote character (0x22)
        // NOT a backslash character (0x5C)
        // Is within the unicode range (< 0x10FFFF) (this is already guaranteed by Rust char)
        (cv >= 0x20) && (cv != 0x22) && (cv != 0x5C)
    });

    // There are only two types of escape allowed by RFC 8259.
    // - single-character escapes \" \\ \/ \b \f \n \r \t
    // - general-purpose \uXXXX
    // Note: we don't enforce that escape codes are valid here.
    // There must be a decoder later on.
    let escape_code = pair(
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
    )
    .recognize();

    many0(alt((nonescaped_string, escape_code)))
        .recognize()
        .delimited_by(tag("\""))
        .map_res::<_, _, UnescapeError>(|s: &str| {
            let unescaped = unescape(s).map_err(|_| UnescapeError(s.to_string()))?;
            Ok(Value::String(unescaped))
        })
        .parse(input)
}

fn rust_ident(input: &str) -> IResult<&str, &str, ErrorTree<&str>> {
    let alpha_or_underscore = anychar.verify(|c: &char| c.is_alpha() || *c == '_');

    take_while1(|c: char| c.is_alphanumeric() || c == '_')
        .preceded_by(alpha_or_underscore.peek())
        .parse(input)
}

/// Parse a signed or unsigned integer literal, supports optional Rust style underscore separators.
fn scon_integer(input: &str) -> IResult<&str, Value, ErrorTree<&str>> {
    let sign = alt((char('+'), char('-')));
    pair(sign.opt(), separated_list0(char('_'), digit1))
        .map_res(|(sign, parts)| {
            let digits = parts.join("");
            if let Some(sign) = sign {
                let s = format!("{}{}", sign, digits);
                s.parse::<i128>().map(Value::Int)
            } else {
                digits.parse::<u128>().map(Value::UInt)
            }
        })
        .parse(input)
}

fn scon_unit(input: &str) -> IResult<&str, Value, ErrorTree<&str>> {
    let (i, _) = tag("()").parse(input)?;
    Ok((i, Value::Unit))
}

fn scon_bool(input: &str) -> IResult<&str, Value, ErrorTree<&str>> {
    alt((
        tag("false").value(Value::Bool(false)),
        tag("true").value(Value::Bool(true)),
    ))
    .parse(input)
}

fn scon_char(input: &str) -> IResult<&str, Value, ErrorTree<&str>> {
    anychar
        .delimited_by(char('\''))
        .map(Value::Char)
        .parse(input)
}

fn scon_seq(input: &str) -> IResult<&str, Value, ErrorTree<&str>> {
    separated_list0(ws(char(',')), scon_value)
        .preceded_by(ws(char('[')))
        .terminated(pair(ws(char(',')).opt(), ws(char(']'))))
        .map(|seq| Value::Seq(seq.into()))
        .parse(input)
}

fn scon_tuple(input: &str) -> IResult<&str, Value, ErrorTree<&str>> {
    let tuple_body = separated_list0(ws(char(',')), scon_value)
        .preceded_by(ws(char('(')))
        .terminated(pair(ws(char(',')).opt(), ws(char(')'))));

    tuple((ws(rust_ident).opt(), tuple_body))
        .map(|(ident, v)| Value::Tuple(Tuple::new(ident, v.into_iter().collect())))
        .parse(input)
}

/// Parse a rust ident on its own which could represent a struct with no fields or a enum unit
/// variant e.g. "None"
fn scon_unit_tuple(input: &str) -> IResult<&str, Value, ErrorTree<&str>> {
    rust_ident
        .map(|ident| Value::Tuple(Tuple::new(Some(ident), Vec::new())))
        .parse(input)
}

fn scon_map(input: &str) -> IResult<&str, Value, ErrorTree<&str>> {
    let opening = alt((tag("("), tag("{")));
    let closing = alt((tag(")"), tag("}")));

    let ident_key = rust_ident.map(|s| Value::String(s.into()));
    let scon_map_key = ws(alt((ident_key, scon_string, scon_integer)));

    let map_body = separated_list0(
        ws(char(',')),
        separated_pair(scon_map_key, ws(char(':')), scon_value),
    )
    .preceded_by(ws(opening))
    .terminated(pair(ws(char(',')).opt(), ws(closing)));

    tuple((ws(rust_ident).opt(), map_body))
        .map(|(ident, v)| Value::Map(Map::new(ident, v.into_iter().collect())))
        .parse(input)
}

fn scon_bytes(input: &str) -> IResult<&str, Value, ErrorTree<&str>> {
    tag("0x")
        .precedes(hex_digit1)
        .map_res::<_, _, hex::FromHexError>(|byte_str| {
            let bytes = Bytes::from_hex_string(byte_str)?;
            Ok(Value::Bytes(bytes))
        })
        .parse(input)
}

/// Parse any alphanumeric literal with more than 39 characters (the length of `u128::MAX`)
///
/// This is suitable for capturing e.g. Base58 encoded literals for Substrate addresses
fn scon_literal(input: &str) -> IResult<&str, Value, ErrorTree<&str>> {
    const MAX_UINT_LEN: usize = 39;
    alphanumeric1
        .verify(|s: &&str| s.len() > MAX_UINT_LEN)
        .recognize()
        .map(|literal: &str| Value::Literal(literal.to_string()))
        .parse(input)
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

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    fn assert_scon_value(input: &str, expected: Value) {
        assert_eq!(scon_value(input).unwrap(), ("", expected));
    }

    #[test]
    fn test_parse_value() {
        assert_eq!(parse_value("true").unwrap(), Value::Bool(true))
    }

    #[test]
    fn test_unit() {
        assert_eq!(scon_value("()").unwrap(), ("", Value::Unit));
    }

    #[test]
    fn test_bool() {
        assert_eq!(scon_bool("false").unwrap(), ("", Value::Bool(false)));
        assert_eq!(scon_bool("true").unwrap(), ("", Value::Bool(true)));
        assert!(scon_bool("foo").is_err());
    }

    #[test]
    fn test_integer() {
        assert_eq!(scon_integer("42").unwrap(), ("", Value::UInt(42)));
        assert_eq!(scon_integer("-123").unwrap(), ("", Value::Int(-123)));
        assert_eq!(scon_integer("+456").unwrap(), ("", Value::Int(456)));
        assert_eq!(scon_integer("0").unwrap(), ("", Value::UInt(0)));
        assert_eq!(scon_integer("01").unwrap(), ("", Value::UInt(1)));
        assert_eq!(
            scon_integer("340282366920938463463374607431768211455").unwrap(),
            ("", Value::UInt(340282366920938463463374607431768211455))
        );

        // underscore separators
        assert_eq!(
            scon_integer("1_000_000").unwrap(),
            ("", Value::UInt(1_000_000))
        );
        assert_eq!(
            scon_integer("-2_000_000").unwrap(),
            ("", Value::Int(-2_000_000))
        );
        assert_eq!(
            scon_integer("+3_000_000").unwrap(),
            ("", Value::Int(3_000_000))
        );
        assert_eq!(
            scon_integer("340_282_366_920_938_463_463_374_607_431_768_211_455").unwrap(),
            ("", Value::UInt(340282366920938463463374607431768211455))
        );

        // too many digits
        assert_matches!(
            scon_integer("3402823669209384634633746074317682114550"),
            Err(nom::Err::Error(_))
        );
        assert_matches!(scon_integer("abc123"), Err(nom::Err::Error(_)));
    }

    #[test]
    fn test_string() {
        // Plain Unicode strings with no escaping
        assert_eq!(
            scon_string(r#""""#).unwrap(),
            ("", Value::String("".into()))
        );
        assert_eq!(
            scon_string(r#""Hello""#).unwrap(),
            ("", Value::String("Hello".into()))
        );
        assert_eq!(
            scon_string(r#""の""#).unwrap(),
            ("", Value::String("の".into()))
        );
        assert_eq!(
            scon_string(r#""𝄞""#).unwrap(),
            ("", Value::String("𝄞".into()))
        );

        // valid 2-character escapes
        assert_eq!(
            scon_string(r#""  \\  ""#).unwrap(),
            ("", Value::String("  \\  ".into()))
        );
        assert_eq!(
            scon_string(r#""  \"  ""#).unwrap(),
            ("", Value::String("  \"  ".into()))
        );

        // valid 6-character escapes
        assert_eq!(
            scon_string(r#""\u0000""#).unwrap(),
            ("", Value::String("\x00".into()))
        );
        assert_eq!(
            scon_string(r#""\u00DF""#).unwrap(),
            ("", Value::String("ß".into()))
        );
        assert_eq!(
            scon_string(r#""\uD834\uDD1E""#).unwrap(),
            ("", Value::String("𝄞".into()))
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
        assert_matches!(scon_string(r#""\ud800""#), Err(nom::Err::Error(_)));
    }

    #[test]
    fn test_seq() {
        assert_eq!(scon_value("[ ]").unwrap(), ("", Value::Seq(vec![].into())));
        assert_eq!(
            scon_value("[ 1 ]").unwrap(),
            ("", Value::Seq(vec![Value::UInt(1)].into()))
        );

        let expected = Value::Seq(vec![Value::UInt(1), Value::String("x".into())].into());
        assert_eq!(scon_value(r#" [ 1 , "x" ] "#).unwrap(), ("", expected));

        let trailing = r#"["a", "b",]"#;
        assert_eq!(
            scon_value(trailing).unwrap(),
            (
                "",
                Value::Seq(
                    vec![Value::String("a".into()), Value::String("b".into())].into()
                )
            )
        );
    }

    #[test]
    fn test_rust_ident() {
        assert_eq!(rust_ident("a").unwrap(), ("", "a"));
        assert_eq!(rust_ident("a:").unwrap(), (":", "a"));
        assert_eq!(rust_ident("Ok").unwrap(), ("", "Ok"));
        assert_eq!(rust_ident("_ok").unwrap(), ("", "_ok"));
        assert_eq!(rust_ident("im_ok").unwrap(), ("", "im_ok"));
        assert_eq!(rust_ident("im_ok_").unwrap(), ("", "im_ok_"));
        assert_eq!(rust_ident("im_ok_123abc").unwrap(), ("", "im_ok_123abc"));
        assert!(rust_ident("1notok").is_err());
    }

    #[test]
    fn test_literal() {
        assert_eq!(
            scon_literal("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY").unwrap(),
            (
                "",
                Value::Literal("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY".into())
            )
        );
        assert_eq!(
            scon_literal("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY").unwrap(),
            (
                "",
                Value::Literal("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY".into())
            )
        );

        assert_scon_value(
            "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
            Value::Literal("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY".into()),
        );
    }

    #[test]
    fn test_map() {
        assert_eq!(
            scon_value("Foo {}").unwrap(),
            ("", Value::Map(Map::new(Some("Foo"), Default::default())))
        );
        assert_eq!(
            scon_value("Foo{}").unwrap(),
            ("", Value::Map(Map::new(Some("Foo"), Default::default())))
        );

        assert_eq!(rust_ident("a:").unwrap(), (":", "a"));

        assert_eq!(
            scon_value(r#"(a: 1)"#).unwrap(),
            (
                "",
                Value::Map(Map::new(
                    None,
                    vec![(Value::String("a".into()), Value::UInt(1)),]
                        .into_iter()
                        .collect()
                ))
            )
        );

        assert_eq!(
            scon_value(r#"A (a: 1, b: "bar")"#).unwrap(),
            (
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
            )
        );

        assert_eq!(
            scon_value(r#"B(a: 1)"#).unwrap(),
            (
                "",
                Value::Map(Map::new(
                    Some("B"),
                    vec![(Value::String("a".into()), Value::UInt(1)),]
                        .into_iter()
                        .collect()
                ))
            )
        );

        assert_eq!(
            scon_value(r#"Struct { a : 1 }"#).unwrap(),
            (
                "",
                Value::Map(Map::new(
                    Some("Struct"),
                    vec![(Value::String("a".into()), Value::UInt(1)),]
                        .into_iter()
                        .collect()
                ))
            )
        );

        let map = r#"Mixed {
            1: "a",
            "b": 2,
            c: true,
        }"#;

        assert_eq!(
            scon_value(map).unwrap(),
            (
                "",
                Value::Map(Map::new(
                    Some("Mixed"),
                    vec![
                        (Value::UInt(1), Value::String("a".into())),
                        (Value::String("b".into()), Value::UInt(2)),
                        (Value::String("c".into()), Value::Bool(true)),
                        // (Value::String("d".into()), Value::Literal("5ALiteral".into())),
                    ]
                    .into_iter()
                    .collect()
                ))
            )
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
        assert_scon_value(
            r#"0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF"#,
            Value::Bytes(vec![255u8; 23].into()),
        );
    }
}

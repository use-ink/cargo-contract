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

use super::{Bytes, Map, Seq, Tuple, Value};
use std::fmt::{Debug, Display, Formatter, LowerHex, Result};

/// Wraps Value for custom Debug impl to provide pretty-printed Display
struct DisplayValue<'a>(&'a Value);

impl<'a> Debug for DisplayValue<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &self.0 {
            Value::Bool(boolean) => <bool as Debug>::fmt(boolean, f),
            Value::Char(character) => <char as Debug>::fmt(character, f),
            Value::UInt(uint) => <u128 as Display>::fmt(uint, f),
            Value::Int(integer) => <i128 as Display>::fmt(integer, f),
            Value::Map(map) => <DisplayMap as Debug>::fmt(&DisplayMap(map), f),
            Value::Tuple(tuple) => <DisplayTuple as Debug>::fmt(&DisplayTuple(tuple), f),
            Value::String(string) => <String as Display>::fmt(string, f),
            Value::Seq(seq) => <DisplaySeq as Debug>::fmt(&DisplaySeq(seq), f),
            Value::Bytes(bytes) => <Bytes as Debug>::fmt(bytes, f),
            Value::Literal(literal) => <String as Display>::fmt(literal, f),
            Value::Unit => write!(f, "()"),
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Value::String(string) => <String as Display>::fmt(string, f),
            value => <DisplayValue as Debug>::fmt(&DisplayValue(value), f),
        }
    }
}

struct DisplayMap<'a>(&'a Map);

impl<'a> Debug for DisplayMap<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.0.ident {
            Some(ref name) => {
                let mut builder = f.debug_struct(name);
                for (name, value) in self.0.map.iter() {
                    builder.field(&format!("{}", name), &DisplayValue(value));
                }
                builder.finish()
            }
            None => {
                let mut builder = f.debug_map();
                for (name, value) in self.0.map.iter() {
                    builder.entry(name, &DisplayValue(value));
                }
                builder.finish()
            }
        }
    }
}

struct DisplayTuple<'a>(&'a Tuple);

impl<'a> Debug for DisplayTuple<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let name = self.0.ident.as_ref().map_or("", |s| s.as_str());
        let mut builder = f.debug_tuple(name);
        for value in self.0.values.iter() {
            builder.field(&DisplayValue(value));
        }
        builder.finish()
    }
}

struct DisplaySeq<'a>(&'a Seq);

impl<'a> Debug for DisplaySeq<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut builder = f.debug_list();
        for elem in &self.0.elems {
            builder.entry(&DisplayValue(elem));
        }
        builder.finish()
    }
}

impl Debug for Bytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{:#x}", self)
    }
}

impl LowerHex for Bytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if f.alternate() {
            write!(f, "0x{}", hex::encode(&self.bytes))
        } else {
            write!(f, "{}", hex::encode(&self.bytes))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_map() {
        let map = Value::Map(Map::new(
            Some("M"),
            vec![(Value::String("a".into()), Value::UInt(1))]
                .into_iter()
                .collect(),
        ));
        assert_eq!(
            r#"M { a: 1 }"#,
            format!("{}", map),
            "non-alternate same line"
        );
        assert_eq!(
            "M {\n    a: 1,\n}",
            format!("{:#}", map),
            "alternate indented (pretty)"
        );
    }
}

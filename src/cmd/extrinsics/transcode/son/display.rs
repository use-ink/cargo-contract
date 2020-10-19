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
    Seq,
    SonOption,
    Tuple,
    Value
};
use std::fmt::{Display, Formatter, Result};

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Value::Bool(boolean) => boolean.fmt(f),
            Value::Char(character) => character.fmt(f),
            Value::UInt(uint) => uint.fmt(f),
            Value::Int(integer) => integer.fmt(f),
            Value::Map(map) => map.fmt(f),
            Value::Tuple(tuple) => tuple.fmt(f),
            Value::Option(option) => option.fmt(f),
            Value::String(string) => string.fmt(f),
            Value::Seq(seq) => seq.fmt(f),
            Value::Bytes(bytes) => bytes.fmt(f),
            Value::Unit => write!(f, "()")
        }
    }
}

impl Display for Map {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if let Some(ref ident) = self.ident {
            write!(f, "{} {{ ", ident)?;
        } else {
            write!(f, "{{ ")?;
        }

        for (name, value) in self.map.iter() {
            write!(f, "{}: {}, ", name, value)?;
        }
        write!(f, " }}")
    }
}

impl Display for Tuple {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if let Some(ref ident) = self.ident {
            write!(f, "{} ( ", ident)?;
        } else {
            write!(f, "( ")?;
        }

        for field in self.values.iter() {
            write!(f, "{}, ", field)?;
        }
        write!(f, " )")
    }
}

impl Display for Seq {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "[ ")?;
        for item in &self.elems {
            write!(f, "{}, ", item)?;
        }
        write!(f, " ]")
    }
}

impl Display for Bytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", hex::encode(&self.bytes))
    }
}

impl Display for SonOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &self.value {
            None => write!(f, "None"),
            Some(value) => write!(f, "Some({})", value)
        }
    }
}


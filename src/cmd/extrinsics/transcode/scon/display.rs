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
use std::fmt::{Debug, Display, Formatter, Result};

impl Debug for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        fn debug_fmt<T>(name: &str, value: &T, f: &mut Formatter<'_>) -> Result
            where T: Debug
        {
            if !f.alternate() {
                write!(f, "{}(", name)?;
            }
            <T as Debug>::fmt(value, f)?;
            if !f.alternate() {
                write!(f, ")")?;
            }
            Ok(())
        }
        match self {
            Value::Bool(boolean) => debug_fmt("Bool", boolean, f),
            Value::Char(character) => debug_fmt("Char",character, f),
            Value::UInt(uint) => debug_fmt("UInt",uint, f),
            Value::Int(integer) => debug_fmt("Int",integer, f),
            Value::Map(map) => debug_fmt("Map",map, f),
            Value::Tuple(tuple) => debug_fmt("Tuple",tuple, f),
            Value::String(string) => debug_fmt("String",string, f),
            Value::Seq(seq) => debug_fmt("Seq",seq, f),
            Value::Bytes(bytes) => debug_fmt("Bytes",bytes, f),
            Value::Unit => write!(f, "()"),
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        <Value as Debug>::fmt(self, f)
    }
}

impl Debug for Map {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.ident {
            Some(ref name) => {
                let mut builder = f.debug_struct(name);
                for (name, value) in self.map.iter() {
                    builder.field(&format!("{:?}", name), value);
                }
                builder.finish()
            }
            None => {
                let mut builder = f.debug_map();
                for (name, value) in self.map.iter() {
                    builder.entry(name, value);
                }
                builder.finish()
            }
        }
    }
}

impl Debug for Tuple {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let name = self.ident.as_ref().map_or("", |s| s.as_str());
        let mut builder = f.debug_tuple(name);
        for value in self.values.iter() {
            builder.field(value);
        }
        builder.finish()
    }
}

impl Debug for Seq {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut builder = f.debug_list();
        for elem in &self.elems {
            builder.entry(elem);
        }
        builder.finish()
    }
}

impl Debug for Bytes {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "0x{}", hex::encode(&self.bytes))
    }
}

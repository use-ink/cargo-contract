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

//! SCALE Object Notation (SCON)

mod display;
mod parse;

use indexmap::IndexMap;

use std::{
    cmp::{Eq, Ordering},
    hash::{Hash, Hasher},
    iter::FromIterator,
    ops::{Index, IndexMut},
    str::FromStr,
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Value {
    Bool(bool),
    Char(char),
    UInt(u128),
    Int(i128),
    Map(Map),
    Tuple(Tuple),
    String(String),
    Seq(Seq),
    Bytes(Bytes),
    Literal(String),
    Unit,
}

impl FromStr for Value {
    type Err = nom::Err<parse::SonParseError>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse::parse_value(s)
    }
}

#[derive(Clone, Debug)]
pub struct Map {
    ident: Option<String>,
    map: IndexMap<Value, Value>,
}

impl Eq for Map {}

impl Hash for Map {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.iter().for_each(|x| x.hash(state));
    }
}

impl Index<&Value> for Map {
    type Output = Value;

    fn index(&self, index: &Value) -> &Self::Output {
        &self.map[index]
    }
}

impl IndexMut<&Value> for Map {
    fn index_mut(&mut self, index: &Value) -> &mut Self::Output {
        self.map.get_mut(index).expect("no entry found for key")
    }
}

impl Ord for Map {
    fn cmp(&self, other: &Map) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

/// Note: equality is only given if both values and order of values match
impl PartialEq for Map {
    fn eq(&self, other: &Map) -> bool {
        if self.map.len() != other.map.len() {
            return false;
        }
        self.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}

impl PartialOrd for Map {
    fn partial_cmp(&self, other: &Map) -> Option<Ordering> {
        self.iter().partial_cmp(other.iter())
    }
}

impl FromIterator<(Value, Value)> for Map {
    fn from_iter<T: IntoIterator<Item = (Value, Value)>>(iter: T) -> Self {
        Map::new(None, IndexMap::from_iter(iter))
    }
}

impl Map {
    /// Creates a new, empty `Map`.
    pub fn new(ident: Option<&str>, map: IndexMap<Value, Value>) -> Map {
        Map {
            ident: ident.map(|s| s.to_string()),
            map,
        }
    }

    pub fn ident(&self) -> Option<String> {
        self.ident.clone()
    }

    /// Iterate all key-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&Value, &Value)> + DoubleEndedIterator {
        self.map.iter()
    }

    /// Returns an iterator over the map's values
    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.map.values()
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Tuple {
    ident: Option<String>,
    values: Vec<Value>,
}

impl From<Vec<Value>> for Tuple {
    fn from(values: Vec<Value>) -> Self {
        Tuple {
            ident: None,
            values,
        }
    }
}

impl Tuple {
    pub fn new(ident: Option<&str>, values: Vec<Value>) -> Self {
        Tuple {
            ident: ident.map(|s| s.into()),
            values,
        }
    }

    pub fn ident(&self) -> Option<String> {
        self.ident.clone()
    }

    /// Returns an iterator over the tuple's values
    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.values.iter()
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Seq {
    elems: Vec<Value>,
}

impl From<Vec<Value>> for Seq {
    fn from(elems: Vec<Value>) -> Self {
        Self::new(elems)
    }
}

impl Seq {
    pub fn new(elems: Vec<Value>) -> Self {
        Seq { elems }
    }

    pub fn elems(&self) -> &[Value] {
        &self.elems
    }

    pub fn len(&self) -> usize {
        self.elems.len()
    }
}

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Bytes {
    bytes: Vec<u8>,
}

impl From<Vec<u8>> for Bytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }
}

impl Bytes {
    pub fn from_hex_string(s: &str) -> Result<Self, hex::FromHexError> {
        let bytes = crate::util::decode_hex(s)?;
        Ok(Self { bytes })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

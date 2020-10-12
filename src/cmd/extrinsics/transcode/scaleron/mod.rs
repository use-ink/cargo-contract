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

mod parse;

use indexmap::IndexMap;

use std::{
    cmp::{Eq, Ordering},
    hash::{Hash, Hasher},
    iter::FromIterator,
    ops::{Index, IndexMut},
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RonValue {
    Bool(bool),
    Char(char),
    Map(RonMap),
    Number(ron::Number),
    Option(Option<Box<RonValue>>),
    String(String),
    Seq(RonSeq),
    Unit,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RonNumber {
    Uint(u128),
    Int(i128),
    // Float(f64),
}

#[derive(Clone, Debug)]
pub struct RonMap {
    ident: Option<String>,
    map: IndexMap<RonValue, RonValue>,
}

impl Eq for RonMap {}

impl Hash for RonMap {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.iter().for_each(|x| x.hash(state));
    }
}

impl Index<&RonValue> for RonMap {
    type Output = RonValue;

    fn index(&self, index: &RonValue) -> &Self::Output {
        &self.map[index]
    }
}

impl IndexMut<&RonValue> for RonMap {
    fn index_mut(&mut self, index: &RonValue) -> &mut Self::Output {
        self.map.get_mut(index).expect("no entry found for key")
    }
}

impl Ord for RonMap {
    fn cmp(&self, other: &RonMap) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

/// Note: equality is only given if both values and order of values match
impl PartialEq for RonMap {
    fn eq(&self, other: &RonMap) -> bool {
        self.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}

impl PartialOrd for RonMap {
    fn partial_cmp(&self, other: &RonMap) -> Option<Ordering> {
        self.iter().partial_cmp(other.iter())
    }
}

impl RonMap {
    /// Creates a new, empty `Map`.
    pub fn new<S>(ident: Option<S>) -> RonMap
    where
        S: AsRef<str>
    {
        RonMap {
            ident: ident.map(|s| s.as_ref().to_string()),
            map: Default::default(),
        }
    }

    /// Returns the underlying RON values map
    pub fn map(&self) -> &IndexMap<RonValue, RonValue> {
        &self.map
    }

    /// Iterate all key-value pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&RonValue, &RonValue)> + DoubleEndedIterator {
        self.map.iter()
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RonSeq {
    ident: Option<String>,
    values: Vec<RonValue>,
}

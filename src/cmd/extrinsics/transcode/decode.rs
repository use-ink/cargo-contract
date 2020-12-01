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
    env_types::{EnvTypesTranscoder, TypeLookupId},
    scon::{Map, Tuple, Value},
    CompositeTypeFields,
};
use anyhow::Result;
use scale::{Compact, Decode, Input};
use scale_info::{
    form::{CompactForm, Form},
    Field, RegistryReadOnly, Type, TypeDef, TypeDefArray, TypeDefComposite, TypeDefPrimitive,
    TypeDefSequence, TypeDefTuple, TypeDefVariant, Variant,
};

pub struct Decoder<'a> {
    registry: &'a RegistryReadOnly,
    env_types: &'a EnvTypesTranscoder,
}

impl<'a> Decoder<'a> {
    pub fn new(registry: &'a RegistryReadOnly, env_types: &'a EnvTypesTranscoder) -> Self {
        Self {
            registry,
            env_types,
        }
    }

    pub fn decode<T>(&self, ty: T, input: &mut &[u8]) -> Result<Value>
    where
        T: Into<TypeLookupId>,
    {
        let type_id = ty.into();
        let ty = self
            .registry
            .resolve(type_id.type_id())
            .ok_or(anyhow::anyhow!(
                "Failed to resolve type with id `{:?}`",
                type_id
            ))?;
        log::debug!(
            "Decoding input with type id `{:?}` and definition `{:?}`",
            type_id,
            ty
        );
        match self.env_types.try_decode(&type_id, input) {
            // Value was decoded with custom decoder for type.
            Ok(Some(value)) => Ok(value),
            // No custom decoder registered so attempt default decoding.
            Ok(None) => ty.type_def().decode_value(self, &ty, input),
            Err(e) => Err(e),
        }
    }

    fn decode_seq(
        &self,
        ty: &<CompactForm as Form>::Type,
        len: usize,
        decoder: &Decoder,
        input: &mut &[u8],
    ) -> Result<Value> {
        let ty = self
            .registry
            .resolve(ty.id())
            .ok_or(anyhow::anyhow!("Failed to find type with id '{}'", ty.id()))?;

        if *ty.type_def() == TypeDef::Primitive(TypeDefPrimitive::U8) {
            let mut bytes = vec![0u8; len];
            input.read(&mut bytes)?;
            Ok(Value::Bytes(bytes.into()))
        } else {
            let mut elems = Vec::new();
            while elems.len() < len as usize {
                let elem = ty.type_def().decode_value(decoder, ty, input)?;
                elems.push(elem)
            }
            Ok(Value::Seq(elems.into()))
        }
    }
}

pub trait DecodeValue {
    fn decode_value(
        &self,
        decoder: &Decoder,
        ty: &Type<CompactForm>,
        input: &mut &[u8],
    ) -> Result<Value>;
}

impl DecodeValue for TypeDef<CompactForm> {
    fn decode_value(
        &self,
        decoder: &Decoder,
        ty: &Type<CompactForm>,
        input: &mut &[u8],
    ) -> Result<Value> {
        match self {
            TypeDef::Composite(composite) => composite.decode_value(decoder, ty, input),
            TypeDef::Tuple(tuple) => tuple.decode_value(decoder, ty, input),
            TypeDef::Variant(variant) => variant.decode_value(decoder, ty, input),
            TypeDef::Array(array) => array.decode_value(decoder, ty, input),
            TypeDef::Sequence(sequence) => sequence.decode_value(decoder, ty, input),
            TypeDef::Primitive(primitive) => primitive.decode_value(decoder, ty, input),
        }
    }
}

impl DecodeValue for TypeDefComposite<CompactForm> {
    fn decode_value(
        &self,
        decoder: &Decoder,
        ty: &Type<CompactForm>,
        input: &mut &[u8],
    ) -> Result<Value> {
        let struct_type = CompositeTypeFields::from_type_def(&self)?;
        let ident = ty.path().segments().last().map(|s| s.as_str());

        match struct_type {
            CompositeTypeFields::StructNamedFields(fields) => {
                let mut map = Vec::new();
                for field in fields {
                    let value = field.field().decode_value(decoder, ty, input)?;
                    map.push((Value::String(field.name().to_string()), value));
                }
                Ok(Value::Map(Map::new(ident, map.into_iter().collect())))
            }
            CompositeTypeFields::TupleStructUnnamedFields(fields) => {
                let mut tuple = Vec::new();
                for field in fields {
                    let value = field.decode_value(decoder, ty, input)?;
                    tuple.push(value);
                }
                Ok(Value::Tuple(Tuple::new(
                    ident,
                    tuple.into_iter().collect::<Vec<_>>(),
                )))
            }
            CompositeTypeFields::NoFields => Ok(Value::Tuple(Tuple::new(ident, Vec::new()))),
        }
    }
}

impl DecodeValue for TypeDefTuple<CompactForm> {
    fn decode_value(
        &self,
        decoder: &Decoder,
        _: &Type<CompactForm>,
        input: &mut &[u8],
    ) -> Result<Value> {
        let mut tuple = Vec::new();
        for field_type in self.fields() {
            let value = decoder.decode(field_type.id(), input)?;
            tuple.push(value);
        }
        Ok(Value::Tuple(Tuple::new(
            None,
            tuple.into_iter().collect::<Vec<_>>(),
        )))
    }
}

impl DecodeValue for TypeDefVariant<CompactForm> {
    fn decode_value(
        &self,
        decoder: &Decoder,
        ty: &Type<CompactForm>,
        input: &mut &[u8],
    ) -> Result<Value> {
        let discriminant = input.read_byte()?;
        let variant = self
            .variants()
            .get(discriminant as usize)
            .ok_or(anyhow::anyhow!(
                "No variant found with discriminant {}",
                discriminant
            ))?;
        variant.decode_value(decoder, ty, input)
    }
}

impl DecodeValue for Variant<CompactForm> {
    fn decode_value(
        &self,
        decoder: &Decoder,
        ty: &Type<CompactForm>,
        input: &mut &[u8],
    ) -> Result<Value> {
        let mut named = Vec::new();
        let mut unnamed = Vec::new();
        for field in self.fields() {
            let value = field.decode_value(decoder, ty, input)?;
            if let Some(name) = field.name() {
                named.push((Value::String(name.to_owned()), value));
            } else {
                unnamed.push(value);
            }
        }
        if !named.is_empty() && !unnamed.is_empty() {
            Err(anyhow::anyhow!(
                "Variant must have either all named or all unnamed fields"
            ))
        } else if !named.is_empty() {
            Ok(Value::Map(Map::new(
                Some(self.name()),
                named.into_iter().collect(),
            )))
        } else {
            Ok(Value::Tuple(Tuple::new(Some(self.name()), unnamed)))
        }
    }
}

impl DecodeValue for Field<CompactForm> {
    fn decode_value(
        &self,
        decoder: &Decoder,
        _: &Type<CompactForm>,
        input: &mut &[u8],
    ) -> Result<Value> {
        decoder.decode(self, input)
    }
}

impl DecodeValue for TypeDefArray<CompactForm> {
    fn decode_value(
        &self,
        decoder: &Decoder,
        _: &Type<CompactForm>,
        input: &mut &[u8],
    ) -> Result<Value> {
        decoder.decode_seq(self.type_param(), self.len() as usize, decoder, input)
    }
}

impl DecodeValue for TypeDefSequence<CompactForm> {
    fn decode_value(
        &self,
        decoder: &Decoder,
        _: &Type<CompactForm>,
        input: &mut &[u8],
    ) -> Result<Value> {
        let len = <Compact<u32>>::decode(input)?;
        decoder.decode_seq(self.type_param(), len.0 as usize, decoder, input)
    }
}

impl DecodeValue for TypeDefPrimitive {
    fn decode_value(&self, _: &Decoder, _: &Type<CompactForm>, input: &mut &[u8]) -> Result<Value> {
        fn decode_uint<T>(input: &mut &[u8]) -> Result<Value>
        where
            T: Decode + Into<u128>,
        {
            let decoded = T::decode(input)?;
            Ok(Value::UInt(decoded.into()))
        }
        fn decode_int<T>(input: &mut &[u8]) -> Result<Value>
        where
            T: Decode + Into<i128>,
        {
            let decoded = T::decode(input)?;
            Ok(Value::Int(decoded.into()))
        }

        match self {
            TypeDefPrimitive::Bool => Ok(Value::Bool(bool::decode(input)?)),
            TypeDefPrimitive::Char => Err(anyhow::anyhow!("scale codec not implemented for char")),
            TypeDefPrimitive::Str => Ok(Value::String(String::decode(input)?)),
            TypeDefPrimitive::U8 => decode_uint::<u8>(input),
            TypeDefPrimitive::U16 => decode_uint::<u16>(input),
            TypeDefPrimitive::U32 => decode_uint::<u32>(input),
            TypeDefPrimitive::U64 => decode_uint::<u64>(input),
            TypeDefPrimitive::U128 => decode_uint::<u128>(input),
            TypeDefPrimitive::U256 => Err(anyhow::anyhow!("U256 currently not supported")),
            TypeDefPrimitive::I8 => decode_int::<i8>(input),
            TypeDefPrimitive::I16 => decode_int::<i16>(input),
            TypeDefPrimitive::I32 => decode_int::<i32>(input),
            TypeDefPrimitive::I64 => decode_int::<i64>(input),
            TypeDefPrimitive::I128 => decode_int::<i128>(input),
            TypeDefPrimitive::I256 => Err(anyhow::anyhow!("I256 currently not supported")),
        }
    }
}

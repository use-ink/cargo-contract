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

use anyhow::Result;
use scale::{Compact, Decode, Input};
use scale_info::{
    form::{CompactForm, Form},
    Field, RegistryReadOnly, Type, TypeDef, TypeDefArray, TypeDefVariant, TypeDefComposite, TypeDefTuple, TypeDefPrimitive,
    TypeDefSequence, Variant,
};
use std::{convert::TryInto, fmt::Debug};
use super::{
    scon::{Map, Tuple, Value},
    CompositeTypeFields,
};
use sp_core::sp_std::num::NonZeroU32;

pub trait DecodeValue {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        ty: &Type<CompactForm>,
        input: &mut I,
    ) -> Result<Value>;
}

pub fn decode_value<I>(registry: &RegistryReadOnly, type_id: NonZeroU32, input: &mut I) -> Result<Value>
where
    I: Input + Debug
{
    let ty = registry.resolve(type_id)
        .ok_or(anyhow::anyhow!(
            "Failed to resolve type with id '{}'",
            type_id
        ))?;
    ty.type_def().decode_value(registry, &ty, input)
}

impl DecodeValue for TypeDef<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        ty: &Type<CompactForm>,
        input: &mut I,
    ) -> Result<Value> {
        match self {
            TypeDef::Composite(composite) => composite.decode_value(registry, ty, input),
            TypeDef::Tuple(tuple) => tuple.decode_value(registry, ty, input),
            TypeDef::Variant(variant) => variant.decode_value(registry, ty, input),
            TypeDef::Array(array) => array.decode_value(registry, ty, input),
            TypeDef::Sequence(sequence) => sequence.decode_value(registry, ty, input),
            TypeDef::Primitive(primitive) => primitive.decode_value(registry, ty, input),
        }
    }
}

impl DecodeValue for TypeDefComposite<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        ty: &Type<CompactForm>,
        input: &mut I,
    ) -> Result<Value> {
        let struct_type = CompositeTypeFields::from_type_def(&self)?;
        let ident = ty.path().segments().last().map(|s| s.as_str());

        match struct_type {
            CompositeTypeFields::StructNamedFields(fields) => {
                let mut map = Vec::new();
                for field in fields {
                    let value = field.field().decode_value(registry, ty, input)?;
                    map.push((Value::String(field.name().to_string()), value));
                }
                Ok(Value::Map(Map::new(ident, map.into_iter().collect())))
            },
            CompositeTypeFields::TupleStructUnnamedFields(fields) => {
                let mut tuple = Vec::new();
                for field in fields {
                    let value = field.decode_value(registry, ty, input)?;
                    tuple.push(value);
                }
                Ok(Value::Tuple(Tuple::new(ident, tuple.into_iter().collect::<Vec<_>>())))
            }
            CompositeTypeFields::NoFields => {
                Ok(Value::Tuple(Tuple::new(ident, Vec::new())))
            }
        }
    }
}

impl DecodeValue for TypeDefTuple<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        _: &Type<CompactForm>,
        input: &mut I,
    ) -> Result<Value> {
        let mut tuple = Vec::new();
        for field_type in self.fields() {
            let value = decode_value(registry, field_type.id(), input)?;
            tuple.push(value);
        }
        Ok(Value::Tuple(Tuple::new(None, tuple.into_iter().collect::<Vec<_>>())))
    }
}

impl DecodeValue for TypeDefVariant<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        ty: &Type<CompactForm>,
        input: &mut I,
    ) -> Result<Value> {
        let discriminant = input.read_byte()?;
        let variant = self.variants().get(discriminant as usize)
            .ok_or(anyhow::anyhow!("No variant found with discriminant {}", discriminant))?;
        variant.decode_value(registry, ty, input)
    }
}

impl DecodeValue for Variant<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        ty: &Type<CompactForm>,
        input: &mut I,
    ) -> Result<Value> {
        let mut named = Vec::new();
        let mut unnamed = Vec::new();
        for field in self.fields() {
            let value = field.decode_value(registry, ty, input)?;
            if let Some(name) = field.name() {
                named.push((Value::String(name.to_owned()), value));
            } else {
                unnamed.push(value);
            }
        }
        if !named.is_empty() && !unnamed.is_empty() {
            Err(anyhow::anyhow!("Variant must have either all named or all unnamed fields"))
        } else if !named.is_empty() {
            Ok(Value::Map(Map::new(Some(self.name()), named.into_iter().collect())))
        } else {
            Ok(Value::Tuple(Tuple::new(Some(self.name()), unnamed)))
        }
    }
}

impl DecodeValue for Field<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        _: &Type<CompactForm>,
        input: &mut I,
    ) -> Result<Value> {
        decode_value(registry, self.ty().id(), input)
    }
}

impl DecodeValue for TypeDefArray<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        _: &Type<CompactForm>,
        input: &mut I,
    ) -> Result<Value> {
        decode_seq(self.type_param(), self.len() as usize, registry, input)
    }
}

impl DecodeValue for TypeDefSequence<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        _: &Type<CompactForm>,
        input: &mut I,
    ) -> Result<Value> {
        let len = <Compact<u32>>::decode(input)?;
        decode_seq(self.type_param(), len.0 as usize, registry, input)
    }
}

fn decode_seq<I: Input + Debug>(
    ty: &<CompactForm as Form>::Type,
    len: usize,
    registry: &RegistryReadOnly,
    input: &mut I,
) -> Result<Value> {
    let ty = registry.resolve(ty.id())
        .ok_or(anyhow::anyhow!("Failed to find type with id '{}'", ty.id()))?;

    if *ty.type_def() == TypeDef::Primitive(TypeDefPrimitive::U8) {
        let mut bytes = vec![0u8; len];
        input.read(&mut bytes)?;
        Ok(Value::Bytes(bytes.into()))
    } else {
        let mut elems = Vec::new();
        while elems.len() < len as usize {
            let elem = ty.type_def().decode_value(registry, ty, input)?;
            elems.push(elem)
        }
        Ok(Value::Seq(elems.into()))
    }
}

impl DecodeValue for TypeDefPrimitive {
    fn decode_value<I: Input + Debug>(
        &self,
        _: &RegistryReadOnly,
        _: &Type<CompactForm>,
        input: &mut I,
    ) -> Result<Value> {
        match self {
            TypeDefPrimitive::Bool => Ok(Value::Bool(bool::decode(input)?)),
            TypeDefPrimitive::Char => Err(anyhow::anyhow!("scale codec not implemented for char")),
            TypeDefPrimitive::Str => Ok(Value::String(String::decode(input)?)),
            TypeDefPrimitive::U8 => Ok(Value::UInt(
                u8::decode(input)?.into(),
            )),
            TypeDefPrimitive::U16 => Ok(Value::UInt(
                u16::decode(input)?.into(),
            )),
            TypeDefPrimitive::U32 => Ok(Value::UInt(
                u32::decode(input)?.into(),
            )),
            TypeDefPrimitive::U64 => {
                let decoded = u64::decode(input)?;
                match decoded.try_into() {
                    Ok(i) => Ok(Value::UInt(i)),
                    Err(_) => Ok(Value::String(format!("{}", decoded))),
                }
            }
            TypeDefPrimitive::U128 => {
                let decoded = u128::decode(input)?;
                match decoded.try_into() {
                    Ok(i) => Ok(Value::UInt(i)),
                    Err(_) => Ok(Value::String(format!("{}", decoded))),
                }
            }
            // TypeDefPrimitive::I8 => Ok(i8::encode(&i8::from_str(arg)?)),
            // TypeDefPrimitive::I16 => Ok(i16::encode(&i16::from_str(arg)?)),
            // TypeDefPrimitive::I32 => Ok(i32::encode(&i32::from_str(arg)?)),
            // TypeDefPrimitive::I64 => Ok(i64::encode(&i64::from_str(arg)?)),
            // TypeDefPrimitive::I128 => Ok(i128::encode(&i128::from_str(arg)?)),
            prim => unimplemented!("{:?}", prim),
        }
    }
}

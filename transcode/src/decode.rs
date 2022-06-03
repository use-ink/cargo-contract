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
    env_types::EnvTypesTranscoder,
    scon::{Map, Tuple, Value},
    CompositeTypeFields,
};
use anyhow::{Context, Result};
use scale::{Compact, Decode, Input};
use scale_info::{
    form::{Form, PortableForm},
    Field, PortableRegistry, Type, TypeDef, TypeDefCompact, TypeDefPrimitive,
    TypeDefVariant,
};

pub struct Decoder<'a> {
    registry: &'a PortableRegistry,
    env_types: &'a EnvTypesTranscoder,
}

impl<'a> Decoder<'a> {
    pub fn new(
        registry: &'a PortableRegistry,
        env_types: &'a EnvTypesTranscoder,
    ) -> Self {
        Self {
            registry,
            env_types,
        }
    }

    pub fn decode(&self, type_id: u32, input: &mut &[u8]) -> Result<Value> {
        let ty = self.registry.resolve(type_id).ok_or_else(|| {
            anyhow::anyhow!("Failed to resolve type with id `{:?}`", type_id)
        })?;
        log::debug!(
            "Decoding input with type id `{:?}` and definition `{:?}`",
            type_id,
            ty
        );
        match self.env_types.try_decode(type_id, input) {
            // Value was decoded with custom decoder for type.
            Ok(Some(value)) => Ok(value),
            // No custom decoder registered so attempt default decoding.
            Ok(None) => self.decode_type(type_id, ty, input),
            Err(e) => Err(e),
        }
    }

    fn decode_seq(
        &self,
        ty: &<PortableForm as Form>::Type,
        len: usize,
        input: &mut &[u8],
    ) -> Result<Value> {
        let type_id = ty.id();
        let ty = self.registry.resolve(type_id).ok_or_else(|| {
            anyhow::anyhow!("Failed to find type with id '{}'", type_id)
        })?;

        let mut elems = Vec::new();
        while elems.len() < len as usize {
            let elem = self.decode_type(type_id, ty, input)?;
            elems.push(elem)
        }
        Ok(Value::Seq(elems.into()))
    }

    fn decode_type(
        &self,
        id: u32,
        ty: &Type<PortableForm>,
        input: &mut &[u8],
    ) -> Result<Value> {
        match ty.type_def() {
            TypeDef::Composite(composite) => {
                let ident = ty.path().segments().last().map(|s| s.as_str());
                self.decode_composite(ident, composite.fields(), input)
            }
            TypeDef::Tuple(tuple) => {
                let mut elems = Vec::new();
                for field_type in tuple.fields() {
                    let value = self.decode(field_type.id(), input)?;
                    elems.push(value);
                }
                Ok(Value::Tuple(Tuple::new(
                    None,
                    elems.into_iter().collect::<Vec<_>>(),
                )))
            }
            TypeDef::Variant(variant) => self.decode_variant_type(variant, input),
            TypeDef::Array(array) => {
                self.decode_seq(array.type_param(), array.len() as usize, input)
            }
            TypeDef::Sequence(sequence) => {
                let len = <Compact<u32>>::decode(input)?;
                self.decode_seq(sequence.type_param(), len.0 as usize, input)
            }
            TypeDef::Primitive(primitive) => self.decode_primitive(primitive, input),
            TypeDef::Compact(compact) => self.decode_compact(compact, input),
            TypeDef::BitSequence(_) => {
                Err(anyhow::anyhow!("bitvec decoding not yet supported"))
            }
        }
        .context(format!("Error decoding type {}: {}", id, ty.path()))
    }

    pub fn decode_composite(
        &self,
        ident: Option<&str>,
        fields: &[Field<PortableForm>],
        input: &mut &[u8],
    ) -> Result<Value> {
        let struct_type = CompositeTypeFields::from_fields(fields)?;

        match struct_type {
            CompositeTypeFields::Named(fields) => {
                let mut map = Vec::new();
                for field in fields {
                    let value = self.decode(field.field().ty().id(), input)?;
                    map.push((Value::String(field.name().to_string()), value));
                }
                Ok(Value::Map(Map::new(ident, map.into_iter().collect())))
            }
            CompositeTypeFields::Unnamed(fields) => {
                let mut tuple = Vec::new();
                for field in &fields {
                    let value = self.decode(field.ty().id(), input)?;
                    tuple.push(value);
                }
                Ok(Value::Tuple(Tuple::new(
                    ident,
                    tuple.into_iter().collect::<Vec<_>>(),
                )))
            }
            CompositeTypeFields::NoFields => {
                Ok(Value::Tuple(Tuple::new(ident, Vec::new())))
            }
        }
    }

    fn decode_variant_type(
        &self,
        variant_type: &TypeDefVariant<PortableForm>,
        input: &mut &[u8],
    ) -> Result<Value> {
        let discriminant = input.read_byte()?;
        let variant = variant_type
            .variants()
            .get(discriminant as usize)
            .ok_or_else(|| {
                anyhow::anyhow!("No variant found with discriminant {}", discriminant)
            })?;

        let mut named = Vec::new();
        let mut unnamed = Vec::new();
        for field in variant.fields() {
            let value = self.decode(field.ty().id(), input)?;
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
                Some(variant.name()),
                named.into_iter().collect(),
            )))
        } else {
            Ok(Value::Tuple(Tuple::new(Some(variant.name()), unnamed)))
        }
    }

    fn decode_primitive(
        &self,
        primitive: &TypeDefPrimitive,
        input: &mut &[u8],
    ) -> Result<Value> {
        match primitive {
            TypeDefPrimitive::Bool => Ok(Value::Bool(bool::decode(input)?)),
            TypeDefPrimitive::Char => {
                Err(anyhow::anyhow!("scale codec not implemented for char"))
            }
            TypeDefPrimitive::Str => Ok(Value::String(String::decode(input)?)),
            TypeDefPrimitive::U8 => decode_uint::<u8>(input),
            TypeDefPrimitive::U16 => decode_uint::<u16>(input),
            TypeDefPrimitive::U32 => decode_uint::<u32>(input),
            TypeDefPrimitive::U64 => decode_uint::<u64>(input),
            TypeDefPrimitive::U128 => decode_uint::<u128>(input),
            TypeDefPrimitive::U256 => {
                Err(anyhow::anyhow!("U256 currently not supported"))
            }
            TypeDefPrimitive::I8 => decode_int::<i8>(input),
            TypeDefPrimitive::I16 => decode_int::<i16>(input),
            TypeDefPrimitive::I32 => decode_int::<i32>(input),
            TypeDefPrimitive::I64 => decode_int::<i64>(input),
            TypeDefPrimitive::I128 => decode_int::<i128>(input),
            TypeDefPrimitive::I256 => {
                Err(anyhow::anyhow!("I256 currently not supported"))
            }
        }
    }

    fn decode_compact(
        &self,
        compact: &TypeDefCompact<PortableForm>,
        input: &mut &[u8],
    ) -> Result<Value> {
        let mut decode_compact_primitive = |primitive: &TypeDefPrimitive| match primitive
        {
            TypeDefPrimitive::U8 => {
                Ok(Value::UInt(Compact::<u8>::decode(input)?.0.into()))
            }
            TypeDefPrimitive::U16 => {
                Ok(Value::UInt(Compact::<u16>::decode(input)?.0.into()))
            }
            TypeDefPrimitive::U32 => {
                Ok(Value::UInt(Compact::<u32>::decode(input)?.0.into()))
            }
            TypeDefPrimitive::U64 => {
                Ok(Value::UInt(Compact::<u64>::decode(input)?.0.into()))
            }
            TypeDefPrimitive::U128 => {
                Ok(Value::UInt(Compact::<u128>::decode(input)?.into()))
            }
            prim => Err(anyhow::anyhow!(
                "{:?} not supported. Expected unsigned int primitive.",
                prim
            )),
        };

        let type_id = compact.type_param().id();
        let ty = self.registry.resolve(type_id).ok_or_else(|| {
            anyhow::anyhow!("Failed to resolve type with id `{:?}`", type_id)
        })?;
        match ty.type_def() {
            TypeDef::Primitive(primitive) => decode_compact_primitive(primitive),
            TypeDef::Composite(composite) => match composite.fields() {
                [field] => {
                    let type_id = field.ty().id();
                    let field_ty = self.registry.resolve(type_id).ok_or_else(|| {
                        anyhow::anyhow!("Failed to resolve type with id `{:?}`", type_id)
                    })?;
                    if let TypeDef::Primitive(primitive) = field_ty.type_def() {
                        let struct_ident =
                            ty.path().segments().last().map(|s| s.as_str());
                        let field_value = decode_compact_primitive(primitive)?;
                        let compact_composite = match field.name() {
                            Some(name) => Value::Map(Map::new(
                                struct_ident,
                                vec![(Value::String(name.to_string()), field_value)]
                                    .into_iter()
                                    .collect(),
                            )),
                            None => {
                                Value::Tuple(Tuple::new(struct_ident, vec![field_value]))
                            }
                        };
                        Ok(compact_composite)
                    } else {
                        Err(anyhow::anyhow!(
                            "Composite type must have a single primitive field"
                        ))
                    }
                }
                _ => Err(anyhow::anyhow!("Composite type must have a single field")),
            },
            _ => Err(anyhow::anyhow!(
                "Compact type must be a primitive or a composite type"
            )),
        }
    }
}

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

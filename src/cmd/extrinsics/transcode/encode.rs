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
    scon::Value,
    CompositeTypeFields,
};
use anyhow::Result;
use itertools::Itertools;
use scale::{
    Compact,
    Encode,
    Output,
};
use scale_info::{
    form::{
        Form,
        PortableForm,
    },
    Field,
    PortableRegistry,
    TypeDef,
    TypeDefCompact,
    TypeDefPrimitive,
    TypeDefTuple,
    TypeDefVariant,
};
use std::{
    convert::{
        TryFrom,
        TryInto,
    },
    error::Error,
    fmt::Debug,
    str::FromStr,
};

pub struct Encoder<'a> {
    registry: &'a PortableRegistry,
    env_types: &'a EnvTypesTranscoder,
}

impl<'a> Encoder<'a> {
    pub fn new(
        registry: &'a PortableRegistry,
        env_types: &'a EnvTypesTranscoder,
    ) -> Self {
        Self {
            registry,
            env_types,
        }
    }

    pub fn encode<O>(&self, type_id: u32, value: &Value, output: &mut O) -> Result<()>
    where
        O: Output + Debug,
    {
        let ty = self.registry.resolve(type_id).ok_or_else(|| {
            anyhow::anyhow!("Failed to resolve type with id '{:?}'", type_id)
        })?;

        log::debug!(
            "Encoding value `{:?}` with type id `{:?}` and definition `{:?}`",
            value,
            type_id,
            ty.type_def(),
        );
        if !self.env_types.try_encode(type_id, value, output)? {
            match ty.type_def() {
                TypeDef::Composite(composite) => {
                    self.encode_composite(composite.fields(), value, output)
                }
                TypeDef::Variant(variant) => {
                    self.encode_variant_type(variant, value, output)
                }
                TypeDef::Array(array) => {
                    self.encode_seq(array.type_param(), value, false, output)
                }
                TypeDef::Tuple(tuple) => self.encode_tuple(tuple, value, output),
                TypeDef::Sequence(sequence) => {
                    self.encode_seq(sequence.type_param(), value, true, output)
                }
                TypeDef::Primitive(primitive) => {
                    self.encode_primitive(primitive, value, output)
                }
                TypeDef::Compact(compact) => self.encode_compact(compact, value, output),
                TypeDef::BitSequence(_) => {
                    Err(anyhow::anyhow!("bitvec encoding not yet supported"))
                }
            }?;
        }
        Ok(())
    }

    fn encode_composite<O: Output + Debug>(
        &self,
        fields: &[Field<PortableForm>],
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        let struct_type = CompositeTypeFields::from_fields(fields)?;

        match value {
            Value::Map(map) => {
                match struct_type {
                    CompositeTypeFields::Unnamed(fields) => {
                        for (field, value) in fields.iter().zip(map.values()) {
                            self.encode(field.ty().id(), value, output)?;
                        }
                        Ok(())
                    }
                    CompositeTypeFields::NoFields => Ok(()),
                    CompositeTypeFields::Named(named_fields) => {
                        for named_field in named_fields {
                            let field_name = named_field.name();
                            let value = map.get_by_str(field_name).ok_or_else(|| {
                                anyhow::anyhow!("Missing a field named `{}`", field_name)
                            })?;
                            self.encode(named_field.field().ty().id(), value, output)
                                .map_err(|e| {
                                    anyhow::anyhow!(
                                        "Error encoding field `{}`: {}",
                                        field_name,
                                        e
                                    )
                                })?;
                        }
                        Ok(())
                    }
                }
            }
            Value::Tuple(tuple) => {
                match struct_type {
                    CompositeTypeFields::Unnamed(fields) => {
                        for (field, value) in fields.iter().zip(tuple.values()) {
                            self.encode(field.ty().id(), value, output)?;
                        }
                        Ok(())
                    }
                    CompositeTypeFields::NoFields => Ok(()),
                    CompositeTypeFields::Named(_) => {
                        return Err(anyhow::anyhow!(
                            "Type is a struct requiring named fields"
                        ))
                    }
                }
            }
            v => {
                if let Ok(single_field) = fields.iter().exactly_one() {
                    self.encode(single_field.ty().id(), value, output)
                } else {
                    Err(anyhow::anyhow!(
                        "Expected a Map or a Tuple or a single Value for a composite data type, found {:?}",
                        v
                    ))
                }
            }
        }
    }

    fn encode_tuple<O: Output + Debug>(
        &self,
        tuple: &TypeDefTuple<PortableForm>,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        match value {
            Value::Tuple(tuple_val) => {
                for (field_type, value) in tuple.fields().iter().zip(tuple_val.values()) {
                    self.encode(field_type.id(), value, output)?;
                }
                Ok(())
            }
            v => {
                if let Ok(single_field) = tuple.fields().iter().exactly_one() {
                    self.encode(single_field.id(), value, output)
                } else {
                    Err(anyhow::anyhow!(
                        "Expected a Tuple or a single Value for a tuple data type, found {:?}",
                        v
                    ))
                }
            }
        }
    }

    fn encode_variant_type<O: Output + Debug>(
        &self,
        variant_def: &TypeDefVariant<PortableForm>,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        let variant_ident = match value {
            Value::Map(map) => {
                map.ident().ok_or_else(|| {
                    anyhow::anyhow!("Missing enum variant identifier for map")
                })
            }
            Value::Tuple(tuple) => {
                tuple.ident().ok_or_else(|| {
                    anyhow::anyhow!("Missing enum variant identifier for tuple")
                })
            }
            v => Err(anyhow::anyhow!("Invalid enum variant value '{:?}'", v)),
        }?;

        let (index, variant) = variant_def
            .variants()
            .iter()
            .find_position(|v| v.name() == &variant_ident)
            .ok_or_else(|| anyhow::anyhow!("No variant '{}' found", variant_ident))?;

        let index: u8 = index
            .try_into()
            .map_err(|_| anyhow::anyhow!("Variant index > 255"))?;
        output.push_byte(index);

        self.encode_composite(variant.fields(), value, output)
    }

    fn encode_seq<O: Output + Debug>(
        &self,
        ty: &<PortableForm as Form>::Type,
        value: &Value,
        encode_len: bool,
        output: &mut O,
    ) -> Result<()> {
        match value {
            Value::Seq(values) => {
                if encode_len {
                    Compact(values.len() as u32).encode_to(output);
                }
                for value in values.elems() {
                    self.encode(ty.id(), value, output)?;
                }
            }
            Value::Hex(hex) => {
                if encode_len {
                    Compact(hex.bytes().len() as u32).encode_to(output);
                }
                for byte in hex.bytes() {
                    output.push_byte(*byte);
                }
            }
            value => {
                return Err(anyhow::anyhow!("{:?} cannot be encoded as an array", value))
            }
        }
        Ok(())
    }

    fn encode_primitive<O: Output + Debug>(
        &self,
        primitive: &TypeDefPrimitive,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        match primitive {
            TypeDefPrimitive::Bool => {
                if let Value::Bool(b) = value {
                    b.encode_to(output);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Expected a bool value"))
                }
            }
            TypeDefPrimitive::Char => {
                Err(anyhow::anyhow!("scale codec not implemented for char"))
            }
            TypeDefPrimitive::Str => {
                if let Value::String(s) = value {
                    s.encode_to(output);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Expected a String value"))
                }
            }
            TypeDefPrimitive::U8 => encode_uint::<u8, O>(value, "u8", output),
            TypeDefPrimitive::U16 => encode_uint::<u16, O>(value, "u16", output),
            TypeDefPrimitive::U32 => encode_uint::<u32, O>(value, "u32", output),
            TypeDefPrimitive::U64 => encode_uint::<u64, O>(value, "u64", output),
            TypeDefPrimitive::U128 => encode_uint::<u128, O>(value, "u128", output),
            TypeDefPrimitive::U256 => {
                Err(anyhow::anyhow!("U256 currently not supported"))
            }
            TypeDefPrimitive::I8 => encode_int::<i8, O>(value, "i8", output),
            TypeDefPrimitive::I16 => encode_int::<i16, O>(value, "i16", output),
            TypeDefPrimitive::I32 => encode_int::<i32, O>(value, "i32", output),
            TypeDefPrimitive::I64 => encode_int::<i64, O>(value, "i64", output),
            TypeDefPrimitive::I128 => encode_int::<i128, O>(value, "i128", output),
            TypeDefPrimitive::I256 => {
                Err(anyhow::anyhow!("I256 currently not supported"))
            }
        }
    }

    fn encode_compact<O: Output + Debug>(
        &self,
        compact: &TypeDefCompact<PortableForm>,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        let mut encode_compact_primitive =
            |primitive: &TypeDefPrimitive, value: &Value| {
                match primitive {
                    TypeDefPrimitive::U8 => {
                        let uint = uint_from_value::<u8>(value, "u8")?;
                        Compact(uint).encode_to(output);
                        Ok(())
                    }
                    TypeDefPrimitive::U16 => {
                        let uint = uint_from_value::<u16>(value, "u16")?;
                        Compact(uint).encode_to(output);
                        Ok(())
                    }
                    TypeDefPrimitive::U32 => {
                        let uint = uint_from_value::<u32>(value, "u32")?;
                        Compact(uint).encode_to(output);
                        Ok(())
                    }
                    TypeDefPrimitive::U64 => {
                        let uint = uint_from_value::<u64>(value, "u64")?;
                        Compact(uint).encode_to(output);
                        Ok(())
                    }
                    TypeDefPrimitive::U128 => {
                        let uint = uint_from_value::<u128>(value, "u128")?;
                        Compact(uint).encode_to(output);
                        Ok(())
                    }
                    _ => {
                        Err(anyhow::anyhow!(
                            "Compact encoding not supported for {:?}",
                            primitive
                        ))
                    }
                }
            };

        let ty = self
            .registry
            .resolve(compact.type_param().id())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to resolve type with id '{:?}'",
                    compact.type_param().id()
                )
            })?;
        match ty.type_def() {
            TypeDef::Primitive(primitive) => encode_compact_primitive(primitive, value),
            TypeDef::Composite(composite) => {
                match composite.fields() {
                    [field] => {
                        let type_id = field.ty().id();
                        let field_ty =
                            self.registry.resolve(type_id).ok_or_else(|| {
                                anyhow::anyhow!(
                                    "Failed to resolve type with id `{:?}`",
                                    type_id
                                )
                            })?;
                        if let TypeDef::Primitive(primitive) = field_ty.type_def() {
                            let field_values: Vec<_> = match value {
                            Value::Map(map) => Ok(map.values().collect()),
                            Value::Tuple(tuple) => Ok(tuple.values().collect()),
                            x => Err(anyhow::anyhow!(
                                "Compact composite value must be a Map or a Tuple. Found {}",
                                x
                            )),
                        }?;
                            if field_values.len() == 1 {
                                let field_value = field_values[0];
                                encode_compact_primitive(primitive, field_value)
                            } else {
                                Err(anyhow::anyhow!(
                                    "Compact composite value must have a single field"
                                ))
                            }
                        } else {
                            Err(anyhow::anyhow!(
                                "Composite type must have a single primitive field"
                            ))
                        }
                    }
                    _ => Err(anyhow::anyhow!("Composite type must have a single field")),
                }
            }
            _ => {
                Err(anyhow::anyhow!(
                    "Compact type must be a primitive or a composite type"
                ))
            }
        }
    }
}

fn uint_from_value<T>(value: &Value, expected: &str) -> Result<T>
where
    T: TryFrom<u128> + TryFromHex + FromStr,
    <T as TryFrom<u128>>::Error: Error + Send + Sync + 'static,
    <T as FromStr>::Err: Error + Send + Sync + 'static,
{
    match value {
        Value::UInt(i) => {
            let uint = (*i).try_into()?;
            Ok(uint)
        }
        Value::String(s) => {
            let sanitized = s.replace(&['_', ','][..], "");
            let uint = T::from_str(&sanitized)?;
            Ok(uint)
        }
        Value::Hex(hex) => {
            let uint = T::try_from_hex(hex.as_str())?;
            Ok(uint)
        }
        _ => {
            Err(anyhow::anyhow!(
                "Expected a {} or a String value, got {}",
                expected,
                value
            ))
        }
    }
}

fn encode_uint<T, O>(value: &Value, expected: &str, output: &mut O) -> Result<()>
where
    T: TryFrom<u128> + TryFromHex + FromStr + Encode,
    <T as TryFrom<u128>>::Error: Error + Send + Sync + 'static,
    <T as FromStr>::Err: Error + Send + Sync + 'static,
    O: Output,
{
    let uint: T = uint_from_value(value, expected)?;
    uint.encode_to(output);
    Ok(())
}

fn encode_int<T, O>(value: &Value, expected: &str, output: &mut O) -> Result<()>
where
    T: TryFrom<i128> + TryFrom<u128> + FromStr + Encode,
    <T as TryFrom<i128>>::Error: Error + Send + Sync + 'static,
    <T as TryFrom<u128>>::Error: Error + Send + Sync + 'static,
    <T as FromStr>::Err: Error + Send + Sync + 'static,
    O: Output,
{
    let int = match value {
        Value::Int(i) => {
            let i: T = (*i).try_into()?;
            Ok(i)
        }
        Value::UInt(u) => {
            let i: T = (*u).try_into()?;
            Ok(i)
        }
        Value::String(s) => {
            let sanitized = s.replace(&['_', ','][..], "");
            let i = T::from_str(&sanitized)?;
            Ok(i)
        }
        _ => {
            Err(anyhow::anyhow!(
                "Expected a {} or a String value, got {}",
                expected,
                value
            ))
        }
    }?;
    int.encode_to(output);
    Ok(())
}

/// Attempt to instantiate a type from its little-endian bytes representation.
pub trait TryFromHex: Sized {
    /// Create a new instance from the little-endian bytes representation.
    fn try_from_hex(hex: &str) -> Result<Self>;
}

macro_rules! impl_try_from_hex {
    ( $($ty:ident),* ) => { $(
        impl TryFromHex for $ty {
            fn try_from_hex(hex: &str) -> Result<Self> {
                $ty::from_str_radix(hex, 16).map_err(Into::into)
            }
        }
    )* }
}

impl_try_from_hex!(u8, u16, u32, u64, u128);

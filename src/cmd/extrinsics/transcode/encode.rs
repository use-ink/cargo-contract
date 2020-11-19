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

use super::{scon::Value, CompositeTypeFields};
use anyhow::Result;
use itertools::Itertools;
use scale::{Compact, Encode, Output};
use scale_info::{
    form::{CompactForm, Form},
    Field, RegistryReadOnly, TypeDef, TypeDefArray, TypeDefComposite, TypeDefPrimitive,
    TypeDefSequence, TypeDefTuple, TypeDefVariant, Variant, Path, TypeInfo, IntoCompact,
};
use sp_core::crypto::AccountId32;
use std::{
    boxed::Box,
    collections::HashMap,
    convert::{TryFrom, TryInto},
    error::Error,
    fmt::Debug,
    num::NonZeroU32,
    str::FromStr,
};

/// Encodes SCON values into SCALE
pub struct Encoder<'a> {
    registry: &'a RegistryReadOnly,
    custom_encoders: HashMap<ScaleInfoTypeId, Box<dyn CustomEncoder>>,
}

impl<'a> Encoder<'a> {
    pub fn new(registry: &'a RegistryReadOnly) -> Self {
        Self { registry, custom_encoders: HashMap::new() }
    }

    // todo: make a builder pattern instead?
    pub fn register_custom_encoder<T>(&mut self, encoder: T) -> Result<()>
    where
        T: CustomEncoder + 'static
    {
        let path = encoder.type_path();

        // use this to extract all the types from the registry, todo: replace once `fn enumerate()` available in scale-info
        let mut enumerated_types = Vec::new(); //Vec<(NonZeroU32, &Type<CompactForm>>
        let mut i = 1;
        while let Some(ty) = self.registry.resolve(NonZeroU32::new(i).unwrap()) {
            enumerated_types.push((NonZeroU32::new(i).unwrap(), ty));
            i += 1;
        }

        // todo: what to do if no type with matching path - probably expected if type not used in contract: WARN?
        let type_id = enumerated_types
            .iter()
            .find_map(|(id, ty)| if ty.path() == &path { Some(id) } else { None }).unwrap();

        self.custom_encoders.insert(*type_id, Box::new(encoder));
        Ok(())
    }

    pub fn encode_value<O>(
        &self,
        type_id: NonZeroU32,
        value: &Value,
        output: &mut O,
    ) -> Result<()>
        where
            O: Output + Debug,
    {
        let ty = self.registry.resolve(type_id).ok_or(anyhow::anyhow!(
            "Failed to resolve type with id '{}'",
            type_id
        ))?;

        log::debug!("Encoding value {:?} with type {:?}", value, ty);
        match self.custom_encoders.get(&type_id) {
            Some(encoder) => {
                log::debug!("Using custom encoder for type {}", type_id);
                let encoded = encoder.encode(value)?;
                output.write(&encoded);
                Ok(())
            },
            None => {
                ty.type_def()
                    .encode_value_to(self, value, output)
                    .map_err(|e| anyhow::anyhow!("Error encoding value for {:?}: {}", ty.path(), e))
            }
        }
    }
}

pub trait EncodeValue {
    fn encode_value_to<O: Output + Debug>(
        &self,
        encoder: &Encoder,
        value: &Value,
        output: &mut O,
    ) -> Result<()>;
}

impl EncodeValue for TypeDef<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        encoder: &Encoder,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        match self {
            TypeDef::Composite(composite) => composite.encode_value_to(encoder, value, output),
            TypeDef::Variant(variant) => variant.encode_value_to(encoder, value, output),
            TypeDef::Array(array) => array.encode_value_to(encoder, value, output),
            TypeDef::Tuple(tuple) => tuple.encode_value_to(encoder, value, output),
            TypeDef::Sequence(sequence) => sequence.encode_value_to(encoder, value, output),
            TypeDef::Primitive(primitive) => primitive.encode_value_to(encoder, value, output),
        }
    }
}

impl EncodeValue for TypeDefComposite<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        encoder: &Encoder,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        let struct_type = CompositeTypeFields::from_type_def(&self)?;

        match value {
            Value::Map(map) => {
                // todo: should lookup via name so that order does not matter
                for (field, value) in self.fields().iter().zip(map.values()) {
                    field.encode_value_to(encoder, value, output)?;
                }
                Ok(())
            }
            Value::Tuple(tuple) => match struct_type {
                CompositeTypeFields::TupleStructUnnamedFields(fields) => {
                    for (field, value) in fields.iter().zip(tuple.values()) {
                        field.encode_value_to(encoder, value, output)?;
                    }
                    Ok(())
                }
                CompositeTypeFields::NoFields => Ok(()),
                CompositeTypeFields::StructNamedFields(_) => {
                    return Err(anyhow::anyhow!("Type is a struct requiring named fields"))
                }
            },
            v => {
                if let Ok(single_field) = self.fields().iter().exactly_one() {
                    single_field.encode_value_to(encoder, value, output)
                } else {
                    Err(anyhow::anyhow!(
                        "Expected a Map or a Tuple or a single Value for a composite data type, found {:?}",
                        v
                    ))
                }
            }
        }
    }
}

impl EncodeValue for TypeDefTuple<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        encoder: &Encoder,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        match value {
            Value::Tuple(tuple) => {
                for (field_type, value) in self.fields().iter().zip(tuple.values()) {
                    encoder.encode_value(field_type.id(), value, output)?;
                }
                Ok(())
            }
            v => {
                if let Ok(single_field) = self.fields().iter().exactly_one() {
                    encoder.encode_value(single_field.id(), value, output)
                } else {
                    Err(anyhow::anyhow!(
                        "Expected a Tuple or a single Value for a tuple data type, found {:?}",
                        v
                    ))
                }
            }
        }
    }
}

impl EncodeValue for TypeDefVariant<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        encoder: &Encoder,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        let variant_ident = match value {
            Value::Map(map) => map
                .ident()
                .ok_or(anyhow::anyhow!("Missing enum variant identifier for map")),
            Value::Tuple(tuple) => tuple
                .ident()
                .ok_or(anyhow::anyhow!("Missing enum variant identifier for tuple")),
            v => Err(anyhow::anyhow!("Invalid enum variant value '{:?}'", v)),
        }?;

        let (index, variant) = self
            .variants()
            .iter()
            .find_position(|v| v.name() == &variant_ident)
            .ok_or(anyhow::anyhow!("No variant '{}' found", variant_ident))?;

        let index: u8 = index
            .try_into()
            .map_err(|_| anyhow::anyhow!("Variant index > 255"))?;
        output.push_byte(index);

        variant.encode_value_to(encoder, value, output)
    }
}

impl EncodeValue for Variant<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        encoder: &Encoder,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        match value {
            Value::Map(_map) => {
                // todo: should lookup via name so that order does not matter
                // for (field, value) in self.fields().iter().zip(map.values()) {
                //     field.encode_value_to(encoder, value, output)?;
                // }
                // Ok(())
                todo!()
            }
            Value::Tuple(tuple) => {
                for (field, value) in self.fields().iter().zip(tuple.values()) {
                    field.encode_value_to(encoder, value, output)?;
                }
                Ok(())
            }
            v => Err(anyhow::anyhow!("Invalid enum variant value '{:?}'", v)),
        }
    }
}

impl EncodeValue for Field<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        encoder: &Encoder,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        encoder.encode_value(self.ty().id(), value, output)
    }
}

impl EncodeValue for TypeDefArray<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        encoder: &Encoder,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        encode_seq(self.type_param(), encoder, value, false, output)
    }
}

impl EncodeValue for TypeDefSequence<CompactForm> {
    fn encode_value_to<O: Output + Debug>(
        &self,
        encoder: &Encoder,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        encode_seq(self.type_param(), encoder, value, true, output)
    }
}

fn encode_seq<O: Output + Debug>(
    ty: &<CompactForm as Form>::Type,
    encoder: &Encoder,
    value: &Value,
    encode_len: bool,
    output: &mut O,
) -> Result<()> {
    let ty = encoder.registry
        .resolve(ty.id())
        .ok_or(anyhow::anyhow!("Failed to find type with id '{}'", ty.id()))?;
    match value {
        Value::Seq(values) => {
            if encode_len {
                Compact(values.len() as u32).encode_to(output);
            }
            for value in values.elems() {
                ty.type_def().encode_value_to(encoder, value, output)?;
            }
        }
        Value::Bytes(bytes) => {
            if encode_len {
                Compact(bytes.bytes().len() as u32).encode_to(output);
            }
            for byte in bytes.bytes() {
                output.push_byte(*byte);
            }
        }
        value => return Err(anyhow::anyhow!("{:?} cannot be encoded as an array", value)),
    }
    Ok(())
}

impl EncodeValue for TypeDefPrimitive {
    fn encode_value_to<O: Output + Debug>(
        &self,
        _: &Encoder,
        value: &Value,
        output: &mut O,
    ) -> Result<()> {
        fn encode_uint<T, O>(value: &Value, expected: &str, output: &mut O) -> Result<()>
        where
            T: TryFrom<u128> + FromStr + Encode,
            <T as TryFrom<u128>>::Error: Error + Send + Sync + 'static,
            <T as FromStr>::Err: Error + Send + Sync + 'static,
            O: Output,
        {
            match value {
                Value::UInt(i) => {
                    let u: T = (*i).try_into()?;
                    u.encode_to(output);
                    Ok(())
                }
                Value::String(s) => {
                    let sanitized = s.replace(&['_', ','][..], "");
                    let u = T::from_str(&sanitized)?;
                    u.encode_to(output);
                    Ok(())
                }
                _ => Err(anyhow::anyhow!(
                    "Expected a {} or a String value, got {}",
                    expected,
                    value
                )),
            }
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
                _ => Err(anyhow::anyhow!(
                    "Expected a {} or a String value, got {}",
                    expected,
                    value
                )),
            }?;
            int.encode_to(output);
            Ok(())
        }

        match self {
            TypeDefPrimitive::Bool => {
                if let Value::Bool(b) = value {
                    b.encode_to(output);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Expected a bool value"))
                }
            }
            TypeDefPrimitive::Char => Err(anyhow::anyhow!("scale codec not implemented for char")),
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
            TypeDefPrimitive::I8 => encode_int::<i8, O>(value, "i8", output),
            TypeDefPrimitive::I16 => encode_int::<i16, O>(value, "i16", output),
            TypeDefPrimitive::I32 => encode_int::<i32, O>(value, "i32", output),
            TypeDefPrimitive::I64 => encode_int::<i64, O>(value, "i64", output),
            TypeDefPrimitive::I128 => encode_int::<i128, O>(value, "i128", output),
        }
    }
}

/// Alias for the unique type identifier assigned in the `scale-info` type registry.
type ScaleInfoTypeId = NonZeroU32;

/// Implement this trait to define custom encoding for a type in a `scale-info` type registry.
pub trait CustomEncoder {
    fn type_path(&self) -> Path<CompactForm>;
    fn encode(&self, value: &Value) -> Result<Vec<u8>>;
}

struct AccountId;

impl CustomEncoder for AccountId {
    fn type_path(&self) -> Path<CompactForm> {
        <ink_env::DefaultEnvironment as ink_env::Environment>::AccountId::type_info()
            .path()
            .clone()
            .into_compact(&mut Default::default())
    }

    fn encode(&self, value: &Value) -> Result<Vec<u8>> {
        let account_id =
            match value {
                Value::Literal(literal) => {
                    AccountId32::from_str(literal)
                        .map_err(|e| anyhow::anyhow!("Error parsing AccountId from literal `{}`: {}", literal, e))?
                }
                Value::String(string) => {
                    AccountId32::from_str(string)
                        .map_err(|e| anyhow::anyhow!("Error parsing AccountId from string '{}': {}", string, e))?
                },
                Value::Bytes(bytes) => {
                    AccountId32::try_from(bytes.bytes())
                        .map_err(|_| anyhow::anyhow!("Error converting bytes `{:?}` to AccountId", bytes))?
                },
                _ => Err(anyhow::anyhow!("Expected a string or a literal for an AccountId"))?
            };
        Ok(account_id.encode())
    }
}

struct Balance;

impl CustomEncoder for Balance {
    fn type_path(&self) -> Path<CompactForm> {
        <ink_env::DefaultEnvironment as ink_env::Environment>::Balance::type_info()
            .path()
            .clone()
            .into_compact(&mut Default::default())
    }

    fn encode(&self, value: &Value) -> Result<Vec<u8>> {
        unimplemented!()
    }
}

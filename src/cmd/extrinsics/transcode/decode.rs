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
    Field, RegistryReadOnly, Type, TypeDef, TypeDefArray, TypeDefComposite, TypeDefPrimitive,
    TypeDefSequence,
};
use std::{convert::TryInto, fmt::Debug};
use super::{
    resolve_type,
    ronext::Value,
};

pub trait DecodeValue {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        input: &mut I,
    ) -> Result<Value>;
}

impl DecodeValue for Type<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        input: &mut I,
    ) -> Result<Value> {
        self.type_def().decode_value(registry, input)
    }
}

impl DecodeValue for TypeDef<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        input: &mut I,
    ) -> Result<Value> {
        match self {
            TypeDef::Composite(composite) => composite.decode_value(registry, input),
            TypeDef::Array(array) => array.decode_value(registry, input),
            TypeDef::Sequence(sequence) => sequence.decode_value(registry, input),
            TypeDef::Primitive(primitive) => primitive.decode_value(registry, input),
            def => unimplemented!("{:?}", def),
        }
    }
}

impl DecodeValue for TypeDefComposite<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        input: &mut I,
    ) -> Result<Value> {
        let mut map = Vec::new();
        for field in self.fields() {
            let value = field.decode_value(registry, input)?;
            let name = field.name().expect("Struct fields always have a name");
            map.push((Value::String(name.to_string()), value));
        }
        Ok(Value::Map(map.into_iter().collect()))
    }
}

impl DecodeValue for Field<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        input: &mut I,
    ) -> Result<Value> {
        let ty = resolve_type(registry, self.ty())?;
        ty.decode_value(registry, input)
    }
}

impl DecodeValue for TypeDefArray<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
        input: &mut I,
    ) -> Result<Value> {
        decode_seq(self.type_param(), self.len() as usize, registry, input)
    }
}

impl DecodeValue for TypeDefSequence<CompactForm> {
    fn decode_value<I: Input + Debug>(
        &self,
        registry: &RegistryReadOnly,
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
    let ty = resolve_type(registry, ty)?;
    if *ty.type_def() == TypeDef::Primitive(TypeDefPrimitive::U8) {
        // byte arrays represented as hex byte strings
        let mut bytes = vec![0u8; len];
        input.read(&mut bytes)?;
        let byte_str = hex::encode(bytes);
        Ok(Value::String(byte_str))
    } else {
        let mut elems = Vec::new();
        while elems.len() < len as usize {
            let elem = ty.decode_value(registry, input)?;
            elems.push(elem)
        }
        Ok(Value::Seq(elems))
    }
}

impl DecodeValue for TypeDefPrimitive {
    fn decode_value<I: Input + Debug>(
        &self,
        _: &RegistryReadOnly,
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

// todo: replace with Value, maybe an enum variant
#[derive(Debug)]
pub struct DecodedEvent {
    pub name: String,
    pub args: Vec<DecodedEventArg>,
}

#[derive(Debug)]
pub struct DecodedEventArg {
    pub name: String,
    pub value: String,
}

//
// fn decode_event(registry: &RegistryReadOnly, input: &[u8]) -> Result<DecodedEvent> {
// 	match self {
// 		TypeDef::Array(array) => {
// 			match resolve_type(registry, array.type_param.id)? {
// 				Type { type_def: TypeDef::Primitive(TypeDefPrimitive::U8), .. } => {
// 					let len = <Compact<u32>>::decode(data)?;
// 					let mut bytes = Vec::new();
// 					for _ in 0..len.0 {
// 						bytes.push(u8::decode(data)?)
// 					}
// 				},
// 				_ => Err(anyhow::anyhow!("Only byte (u8) arrays supported")),
// 			}
// 		},
// 		TypeDef::Primitive(primitive) => primitive.encode_arg(registry, arg),
// 		TypeDef::Composite(composite) => composite.encode_arg(registry, arg),
// 		_ => unimplemented!(),
// 	}
// }

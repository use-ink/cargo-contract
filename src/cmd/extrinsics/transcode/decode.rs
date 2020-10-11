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
use scale::Decode;
use scale_info::{form::CompactForm, RegistryReadOnly, Type, TypeDef, TypeDefPrimitive, TypeDefArray};
use std::convert::TryInto;
use crate::cmd::extrinsics::transcode::resolve_type;

pub trait DecodeValue {
	fn decode_value(&self, registry: &RegistryReadOnly, input: &mut &[u8]) -> Result<ron::Value>;
}

impl DecodeValue for Type<CompactForm> {
	fn decode_value(&self, registry: &RegistryReadOnly, input: &mut &[u8]) -> Result<ron::Value> {
		self.type_def().decode_value(registry, input)
	}
}

impl DecodeValue for TypeDef<CompactForm> {
	fn decode_value(&self, registry: &RegistryReadOnly, input: &mut &[u8]) -> Result<ron::Value> {
		match self {
			TypeDef::Array(array) => array.decode_value(registry, input),
			TypeDef::Primitive(primitive) => primitive.decode_value(registry, input),
			def => unimplemented!("{:?}", def),
		}
	}
}

impl DecodeValue for TypeDefPrimitive {
	fn decode_value(&self, _: &RegistryReadOnly, input: &mut &[u8]) -> Result<ron::Value> {
		match self {
			TypeDefPrimitive::Bool => Ok(ron::Value::Bool(bool::decode(&mut &input[..])?)),
			TypeDefPrimitive::Char => Err(anyhow::anyhow!("scale codec not implemented for char")),
			TypeDefPrimitive::Str => Ok(ron::Value::String(String::decode(&mut &input[..])?)),
			TypeDefPrimitive::U8 => Ok(ron::Value::Number(ron::Number::Integer(u8::decode(&mut &input[..])?.into()))),
			TypeDefPrimitive::U16 => Ok(ron::Value::Number(ron::Number::Integer(u16::decode(&mut &input[..])?.into()))),
			TypeDefPrimitive::U32 => Ok(ron::Value::Number(ron::Number::Integer(u32::decode(&mut &input[..])?.into()))),
			TypeDefPrimitive::U64 => {
				let decoded = u64::decode(&mut &input[..])?;
				match decoded.try_into() {
					Ok(i) => Ok(ron::Value::Number(ron::Number::Integer(i))),
					Err(_) => Ok(ron::Value::String(format!("{}", decoded))),
				}
			},
			TypeDefPrimitive::U128 => {
				let decoded = u128::decode(&mut &input[..])?;
				match decoded.try_into() {
					Ok(i) => Ok(ron::Value::Number(ron::Number::Integer(i))),
					Err(_) => Ok(ron::Value::String(format!("{}", decoded))),
				}
			},
			// TypeDefPrimitive::I8 => Ok(i8::encode(&i8::from_str(arg)?)),
			// TypeDefPrimitive::I16 => Ok(i16::encode(&i16::from_str(arg)?)),
			// TypeDefPrimitive::I32 => Ok(i32::encode(&i32::from_str(arg)?)),
			// TypeDefPrimitive::I64 => Ok(i64::encode(&i64::from_str(arg)?)),
			// TypeDefPrimitive::I128 => Ok(i128::encode(&i128::from_str(arg)?)),
			prim => unimplemented!("{:?}", prim),
		}
	}
}

impl DecodeValue for TypeDefArray<CompactForm> {
	fn decode_value(&self, registry: &RegistryReadOnly, input: &mut &[u8]) -> Result<ron::Value> {
		let ty = resolve_type(registry, self.type_param())?;
		if *ty.type_def() == TypeDef::Primitive(TypeDefPrimitive::U8) {
			// byte arrays represented as hex byte strings
			let byte_str = hex::encode(&input[..self.len() as usize]);
			Ok(ron::Value::String(byte_str))
		} else {
			let mut elems = Vec::new();
			while elems.len() < self.len() as usize {
				let elem = ty.decode_value(registry, input)?;
				elems.push(elem)
			}
			Ok(ron::Value::Seq(elems))
		}
	}
}



// todo: replace with ron::Value, maybe an enum variant
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

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
use scale::{Encode, Output};
use scale_info::{
	form::CompactForm,
	RegistryReadOnly, Type, TypeDef, TypeDefArray, TypeDefComposite, TypeDefPrimitive,
};
use ron::Value;
use std::convert::TryInto;

use super::resolve_type;

pub trait EncodeValue {
	fn encode_value_to<O: Output>(&self, registry: &RegistryReadOnly, value: &ron::Value, output: &mut O) -> Result<()>;
}

impl EncodeValue for Type<CompactForm> {
	fn encode_value_to<O: Output>(&self, registry: &RegistryReadOnly, value: &Value, output: &mut O) -> Result<()> {
		self.type_def().encode_value_to(registry, value, output)
	}
}

impl EncodeValue for TypeDef<CompactForm> {
	fn encode_value_to<O: Output>(&self, registry: &RegistryReadOnly, value: &Value, output: &mut O) -> Result<()> {
		match self {
			TypeDef::Array(array) => array.encode_value_to(registry, value, output),
			TypeDef::Primitive(primitive) => primitive.encode_value_to(registry, value, output),
			TypeDef::Composite(composite) => composite.encode_value_to(registry, value, output),
			_ => unimplemented!("TypeDef::encode_value"),
		}
		// let ron_value: ron::Value = ron::from_str(arg.as_ref())?;
		// match (ty, ron_value) {
		//     (TypeDef::Primitive(TypeDefPrimitive::Bool), ron::Value::Bool(b)) => Ok(b.encode()),
		//     _ => unimplemented!("encoded types"),
		// }
		// match self {
		//     TypeDef::Array(array) => {
		//         let ty = resolve_type(registry, array.type_param())?;
		//         match ty.type_def() {
		//             TypeDef::Primitive(TypeDefPrimitive::U8) => Ok(hex::decode(arg)?),
		//             _ => Err(anyhow::anyhow!("Only byte (u8) arrays supported")),
		//         }
		//     }
		//     TypeDef::Primitive(primitive) => primitive.encode_value_to(registry, arg),
		//     TypeDef::Composite(composite) => composite.encode_value_to(registry, arg),
		//     _ => unimplemented!(),
		// }
	}
}

impl EncodeValue for TypeDefArray<CompactForm> {
	fn encode_value_to<O: Output>(&self, _registry: &RegistryReadOnly, _value: &Value, _output: &mut O) -> Result<()> {
		unimplemented!();
	}
}

impl EncodeValue for TypeDefPrimitive {
	fn encode_value_to<O: Output>(&self, _: &RegistryReadOnly, value: &Value, output: &mut O) -> Result<()> {
		match self {
			TypeDefPrimitive::Bool => {
				if let ron::Value::Bool(b) = value {
					b.encode_to(output);
					Ok(())
				} else {
					Err(anyhow::anyhow!("Expected a bool value"))
				}
			},
			TypeDefPrimitive::Char => Err(anyhow::anyhow!("scale codec not implemented for char")),
			TypeDefPrimitive::Str => {
				if let ron::Value::String(s) = value {
					s.encode_to(output);
					Ok(())
				} else {
					Err(anyhow::anyhow!("Expected a String value"))
				}
			},
			TypeDefPrimitive::U8 => {
				if let ron::Value::Number(ron::Number::Integer(i)) = value {
					let u: u8 = (*i).try_into()?;
					u.encode_to(output);
					Ok(())
				} else {
					Err(anyhow::anyhow!("Expected a u8 value"))
				}
			},
			TypeDefPrimitive::U16 => {
				if let ron::Value::Number(ron::Number::Integer(i)) = value {
					let u: u16 = (*i).try_into()?;
					u.encode_to(output);
					Ok(())
				} else {
					Err(anyhow::anyhow!("Expected a u16 value"))
				}
			}
			_ => unimplemented!("TypeDefPrimitive::encode_value"),
			// TypeDefPrimitive::U16 => Ok(u16::encode(&u16::from_str(arg)?)),
			// TypeDefPrimitive::U32 => Ok(u32::encode(&u32::from_str(arg)?)),
			// TypeDefPrimitive::U64 => Ok(u64::encode(&u64::from_str(arg)?)),
			// TypeDefPrimitive::U128 => Ok(u128::encode(&u128::from_str(arg)?)),
			// TypeDefPrimitive::I8 => Ok(i8::encode(&i8::from_str(arg)?)),
			// TypeDefPrimitive::I16 => Ok(i16::encode(&i16::from_str(arg)?)),
			// TypeDefPrimitive::I32 => Ok(i32::encode(&i32::from_str(arg)?)),
			// TypeDefPrimitive::I64 => Ok(i64::encode(&i64::from_str(arg)?)),
			// TypeDefPrimitive::I128 => Ok(i128::encode(&i128::from_str(arg)?)),
		}
	}
}

impl EncodeValue for TypeDefComposite<CompactForm> {
	fn encode_value_to<O: Output>(&self, registry: &RegistryReadOnly, value: &Value, output: &mut O) -> Result<()> {
		if self.fields().len() != 1 {
			panic!("Only single field structs currently supported")
		}
		let field = self.fields().iter().next().unwrap();
		if field.name().is_none() {
			let ty = resolve_type(registry, field.ty())?;
			ty.encode_value_to(registry, value, output)
		} else {
			panic!("Only tuple structs currently supported")
		}
	}
}

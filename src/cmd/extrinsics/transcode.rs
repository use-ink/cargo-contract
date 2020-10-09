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
use ink_metadata::{ConstructorSpec, InkProject, MessageSpec};
use scale::{Decode, Encode, Input, Output};
use scale_info::{
    form::{CompactForm, Form},
    RegistryReadOnly, Type, TypeDef, TypeDefArray, TypeDefComposite, TypeDefPrimitive,
};
use ron::Value;
use std::convert::TryInto;

/// Encode strings to SCALE encoded smart contract calls.
/// Decode SCALE encoded smart contract events and return values into `Value` objects.
pub struct Transcoder {
    metadata: InkProject,
}

impl Transcoder {
    pub fn new(metadata: InkProject) -> Self {
        Self { metadata }
    }

    fn registry(&self) -> &RegistryReadOnly {
        self.metadata.registry()
    }

    pub fn encode<I, S>(&self, name: &str, args: I) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let (selector, spec_args) = match (
            self.find_constructor_spec(name),
            self.find_message_spec(name),
        ) {
            (Some(c), None) => (c.selector(), c.args()),
            (None, Some(m)) => (m.selector(), m.args()),
            (Some(_), Some(_)) => {
                return Err(anyhow::anyhow!(
                    "Invalid metadata: both a constructor and message found with name '{}'",
                    name
                ))
            }
            (None, None) => {
                return Err(anyhow::anyhow!(
                    "No constructor or message with the name '{}' found",
                    name
                ))
            }
        };

        let mut encoded = selector.to_bytes().to_vec();
        for (spec, arg) in spec_args.iter().zip(args) {
            let ty = resolve_type(self.registry(), spec.ty().ty())?;
            let value = ron::from_str(arg.as_ref())?;
            ty.encode_value_to(&self.registry(), &value, &mut encoded)?;
        }
        Ok(encoded)
    }

    fn constructors(&self) -> impl Iterator<Item = &ConstructorSpec<CompactForm>> {
        self.metadata.spec().constructors().iter()
    }

    fn messages(&self) -> impl Iterator<Item = &MessageSpec<CompactForm>> {
        self.metadata.spec().messages().iter()
    }

    fn find_message_spec(&self, name: &str) -> Option<&MessageSpec<CompactForm>> {
        self.messages()
            .find(|msg| msg.name().contains(&name.to_string()))
    }

    fn find_constructor_spec(&self, name: &str) -> Option<&ConstructorSpec<CompactForm>> {
        self.constructors()
            .find(|msg| msg.name().contains(&name.to_string()))
    }

    pub fn decode_events<I>(&self, data: &mut I) -> Result<DecodedEvent>
    where
        I: Input,
    {
        let variant_index = data.read_byte()?;
        let event_spec = self
            .metadata
            .spec()
            .events()
            .get(variant_index as usize)
            .ok_or(anyhow::anyhow!(
                "Event variant {} not found in contract metadata",
                variant_index
            ))?;
        let mut args = Vec::new();
        for arg in event_spec.args() {
            args.push(DecodedEventArg {
                name: arg.name().to_string(),
                value: "TODO".to_string(), // todo: resolve and decode type
            })
        }

        Ok(DecodedEvent {
            name: event_spec.name().to_string(),
            args,
        })
    }

    pub fn decode_return(&self, name: &str, data: Vec<u8>) -> Result<ron::Value> {
        let msg_spec = self.find_message_spec(name).ok_or(anyhow::anyhow!(
            "Faiedl to find message spec with name '{}'",
            name
        ))?;
        if let Some(return_ty) = msg_spec.return_type().opt_type() {
            let ty = resolve_type(&self.registry(), return_ty.ty())?;
            ty.type_def()
                .decode_value(self.registry(), &mut &data[..])
        } else {
            Ok(ron::Value::Unit)
        }
    }
}

fn resolve_type(
    registry: &RegistryReadOnly,
    symbol: &<CompactForm as Form>::Type,
) -> Result<Type<CompactForm>> {
    let ty = registry.resolve(symbol.id()).ok_or(anyhow::anyhow!(
        "Failed to resolve type with id '{}'",
        symbol.id()
    ))?;
    Ok(ty.clone())
}

pub trait EncodeValue {
    // todo: rename
    fn encode_value_to<O: Output>(&self, registry: &RegistryReadOnly, value: &ron::Value, output: &mut O) -> Result<()>;
}

pub trait DecodeValue {
    fn decode_value(&self, registry: &RegistryReadOnly, input: &mut &[u8]) -> Result<ron::Value>;
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

impl DecodeValue for TypeDef<CompactForm> {
    fn decode_value(&self, registry: &RegistryReadOnly, input: &mut &[u8]) -> Result<ron::Value> {
        match self {
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
            // TypeDefPrimitive::U16 => Ok(u16::encode(&u16::from_str(arg)?)),
            // TypeDefPrimitive::U32 => Ok(u32::encode(&u32::from_str(arg)?)),
            // TypeDefPrimitive::U64 => Ok(u64::encode(&u64::from_str(arg)?)),
            // TypeDefPrimitive::U128 => Ok(u128::encode(&u128::from_str(arg)?)),
            // TypeDefPrimitive::I8 => Ok(i8::encode(&i8::from_str(arg)?)),
            // TypeDefPrimitive::I16 => Ok(i16::encode(&i16::from_str(arg)?)),
            // TypeDefPrimitive::I32 => Ok(i32::encode(&i32::from_str(arg)?)),
            // TypeDefPrimitive::I64 => Ok(i64::encode(&i64::from_str(arg)?)),
            // TypeDefPrimitive::I128 => Ok(i128::encode(&i128::from_str(arg)?)),
            prim => unimplemented!("{:?}", prim),
        }
    }
}

#[derive(Debug)]
pub struct DecodedEvent {
    name: String,
    args: Vec<DecodedEventArg>,
}

#[derive(Debug)]
pub struct DecodedEventArg {
    name: String,
    value: String,
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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use scale_info::{Registry, MetaType};
    use std::{
        convert::TryFrom,
        num::NonZeroU32,
    };

    use ink_lang as ink;

    #[ink::contract]
    pub mod flipper {
        #[ink(storage)]
        pub struct Flipper {
            value: bool,
        }

        impl Flipper {
            /// Creates a new flipper smart contract initialized with the given value.
            #[ink(constructor)]
            pub fn new(init_value: bool) -> Self {
                Self { value: init_value }
            }

            /// Creates a new flipper smart contract initialized to `false`.
            #[ink(constructor)]
            pub fn default() -> Self {
                Self::new(Default::default())
            }

            /// Flips the current value of the Flipper's bool.
            #[ink(message)]
            pub fn flip(&mut self) {
                self.value = !self.value;
            }

            /// Returns the current value of the Flipper's bool.
            #[ink(message)]
            pub fn get(&self) -> bool {
                self.value
            }
        }
    }

    fn generate_metadata() -> ink_metadata::InkProject {
        extern "Rust" {
            fn __ink_generate_metadata() -> ink_metadata::InkProject;
        }
        unsafe { __ink_generate_metadata() }
    }

    #[test]
    fn encode_single_primitive_arg() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = Transcoder::new(metadata);

        let encoded = transcoder.encode("new", &["true"])?;
        // encoded args follow the 4 byte selector
        let encoded_args = &encoded[4..];

        assert_eq!(true.encode(), encoded_args);
        Ok(())
    }

    #[test]
    fn decode_primitive_return() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = Transcoder::new(metadata);

        let encoded = true.encode();
        let decoded = transcoder.decode_return("get", encoded)?;

        assert_eq!(ron::Value::Bool(true), decoded);
        Ok(())
    }

    fn registry_with_type<T>() -> Result<(RegistryReadOnly, TypeDef<CompactForm>)>
    where
        T: scale_info::TypeInfo + 'static
    {
        let mut registry = Registry::new();
        registry.register_type(&MetaType::new::<T>());
        let registry: RegistryReadOnly = registry.into();

        let ty = registry.resolve(NonZeroU32::try_from(1)?).unwrap();
        let type_def = ty.type_def().clone();
        Ok((registry, type_def))
    }

    fn transcode_roundtrip<T>(input: &str, expected_output: ron::Value) -> Result<()>
    where
        T: scale_info::TypeInfo + 'static
    {
        let (registry, ty) = registry_with_type::<T>()?;

        let value = ron::from_str(input).context("Invalid RON value")?;
        let mut output = Vec::new();
        ty.encode_value_to(&registry, &value, &mut output)?;
        let decoded = ty.decode_value(&registry, &mut &output[..])?;
        assert_eq!(expected_output, decoded);
        Ok(())
    }

    #[test]
    fn transcode_bool() -> Result<()> {
        transcode_roundtrip::<bool>("true", ron::Value::Bool(true))?;
        transcode_roundtrip::<bool>("false", ron::Value::Bool(false))
    }

    #[test]
    fn transcode_char_unsupported() -> Result<()> {
        let (registry, ty) = registry_with_type::<char>()?;

        let encoded = u32::from('c').encode();

        assert!(ty.encode_value_to(&registry, &ron::Value::Char('c'), &mut Vec::new()).is_err());
        assert!(ty.decode_value(&registry, &mut &encoded[..]).is_err());
        Ok(())
    }

    #[test]
    fn transcode_str() -> Result<()> {
        transcode_roundtrip::<String>("\"ink!\"", ron::Value::String("ink!".to_string()))
    }

    #[test]
    fn transcode_unsigned_integers() -> Result<()> {
        transcode_roundtrip::<u8>("0", ron::Value::Number(ron::Number::Integer(0)))?;
        transcode_roundtrip::<u8>("255", ron::Value::Number(ron::Number::Integer(255)))?;

        transcode_roundtrip::<u16>("0", ron::Value::Number(ron::Number::Integer(0)))?;
        transcode_roundtrip::<u16>("65535", ron::Value::Number(ron::Number::Integer(65535)))
    }
}

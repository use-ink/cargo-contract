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
use scale::{Decode, Encode, Input};
use scale_info::{
    form::{CompactForm, Form},
    RegistryReadOnly, Type, TypeDef, TypeDefComposite, TypeDefPrimitive,
};
use std::str::FromStr;

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

        let mut encoded_args = spec_args
            .iter()
            .zip(args)
            .map(|(spec, arg)| {
                let ty = resolve_type(self.registry(), spec.ty().ty())?;
                self.encode_ron(ty.type_def(), arg)
                // ty.type_def().encode_arg(&self.registry(), arg.as_ref())
            })
            .collect::<Result<Vec<_>>>()?
            .concat();
        let mut encoded = selector.to_bytes().to_vec();
        encoded.append(&mut encoded_args);
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

    fn encode_ron<S>(&self, ty: &TypeDef<CompactForm>, arg: S) -> Result<Vec<u8>>
    where
        S: AsRef<str>
    {
        let ron_value: ron::Value = ron::from_str(arg.as_ref())?;
        match (ty, ron_value) {
            (TypeDef::Primitive(TypeDefPrimitive::Bool), ron::Value::Bool(b)) => Ok(b.encode()),
            _ => unimplemented!("encoded types"),
        }
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

pub trait EncodeContractArg {
    // todo: rename
    fn encode_arg(&self, registry: &RegistryReadOnly, arg: &str) -> Result<Vec<u8>>;
}

pub trait DecodeValue {
    fn decode_value(&self, registry: &RegistryReadOnly, input: &mut &[u8]) -> Result<ron::Value>;
}

impl EncodeContractArg for TypeDef<CompactForm> {
    fn encode_arg(&self, registry: &RegistryReadOnly, arg: &str) -> Result<Vec<u8>> {
        match self {
            TypeDef::Array(array) => {
                let ty = resolve_type(registry, array.type_param())?;
                match ty.type_def() {
                    TypeDef::Primitive(TypeDefPrimitive::U8) => Ok(hex::decode(arg)?),
                    _ => Err(anyhow::anyhow!("Only byte (u8) arrays supported")),
                }
            }
            TypeDef::Primitive(primitive) => primitive.encode_arg(registry, arg),
            TypeDef::Composite(composite) => composite.encode_arg(registry, arg),
            _ => unimplemented!(),
        }
    }
}

impl EncodeContractArg for TypeDefPrimitive {
    fn encode_arg(&self, _: &RegistryReadOnly, arg: &str) -> Result<Vec<u8>> {
        match self {
            TypeDefPrimitive::Bool => Ok(bool::encode(&bool::from_str(arg)?)),
            TypeDefPrimitive::Char => unimplemented!("scale codec not implemented for char"),
            TypeDefPrimitive::Str => Ok(str::encode(arg)),
            TypeDefPrimitive::U8 => Ok(u8::encode(&u8::from_str(arg)?)),
            TypeDefPrimitive::U16 => Ok(u16::encode(&u16::from_str(arg)?)),
            TypeDefPrimitive::U32 => Ok(u32::encode(&u32::from_str(arg)?)),
            TypeDefPrimitive::U64 => Ok(u64::encode(&u64::from_str(arg)?)),
            TypeDefPrimitive::U128 => Ok(u128::encode(&u128::from_str(arg)?)),
            TypeDefPrimitive::I8 => Ok(i8::encode(&i8::from_str(arg)?)),
            TypeDefPrimitive::I16 => Ok(i16::encode(&i16::from_str(arg)?)),
            TypeDefPrimitive::I32 => Ok(i32::encode(&i32::from_str(arg)?)),
            TypeDefPrimitive::I64 => Ok(i64::encode(&i64::from_str(arg)?)),
            TypeDefPrimitive::I128 => Ok(i128::encode(&i128::from_str(arg)?)),
        }
    }
}

impl EncodeContractArg for TypeDefComposite<CompactForm> {
    fn encode_arg(&self, registry: &RegistryReadOnly, arg: &str) -> Result<Vec<u8>> {
        if self.fields().len() != 1 {
            panic!("Only single field structs currently supported")
        }
        let field = self.fields().iter().next().unwrap();
        if field.name().is_none() {
            let ty = resolve_type(registry, field.ty())?;
            ty.type_def().encode_arg(registry, arg)
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
            prim => unimplemented!("{:?}", prim),
            // TypeDefPrimitive::Char => unimplemented!("scale codec not implemented for char"),
            // TypeDefPrimitive::Str => Ok(str::encode(arg)),
            // TypeDefPrimitive::U8 => Ok(u8::encode(&u8::from_str(arg)?)),
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
    fn encode_bool_arg() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = Transcoder::new(metadata);

        let encoded = transcoder.encode("new", &["true"])?;
        // encoded args follow the 4 byte selector
        let encoded_args = &encoded[4..];

        assert_eq!(true.encode(), encoded_args);
        Ok(())
    }

    #[test]
    fn decode_bool_return() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = Transcoder::new(metadata);

        let encoded = true.encode();
        let decoded = transcoder.decode_return("get", encoded)?;

        assert_eq!(ron::Value::Bool(true), decoded);
        Ok(())
    }
}

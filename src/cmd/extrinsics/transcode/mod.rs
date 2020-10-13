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

mod decode;
mod encode;
mod ronext;

use self::{
    decode::{DecodeValue, DecodedEvent, DecodedEventArg},
    encode::EncodeValue,
};

use anyhow::Result;
use ink_metadata::{ConstructorSpec, InkProject, MessageSpec};
use scale::Input;
use scale_info::{
    form::{CompactForm, Form},
    RegistryReadOnly, Type,
};

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
            "Failed to find message spec with name '{}'",
            name
        ))?;
        if let Some(return_ty) = msg_spec.return_type().opt_type() {
            let ty = resolve_type(&self.registry(), return_ty.ty())?;
            ty.type_def().decode_value(self.registry(), &mut &data[..])
        } else {
            Ok(ron::Value::Unit)
        }
    }
}

pub fn resolve_type(
    registry: &RegistryReadOnly,
    symbol: &<CompactForm as Form>::Type,
) -> Result<Type<CompactForm>> {
    let ty = registry.resolve(symbol.id()).ok_or(anyhow::anyhow!(
        "Failed to resolve type with id '{}'",
        symbol.id()
    ))?;
    Ok(ty.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use ron::{Number, Value};
    use scale::Encode;
    use scale_info::{MetaType, Registry, TypeDef, TypeInfo};
    use std::{convert::TryFrom, num::NonZeroU32};

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

        assert_eq!(Value::Bool(true), decoded);
        Ok(())
    }

    fn registry_with_type<T>() -> Result<(RegistryReadOnly, TypeDef<CompactForm>)>
    where
        T: scale_info::TypeInfo + 'static,
    {
        let mut registry = Registry::new();
        registry.register_type(&MetaType::new::<T>());
        let registry: RegistryReadOnly = registry.into();

        let ty = registry.resolve(NonZeroU32::try_from(1)?).unwrap();
        let type_def = ty.type_def().clone();
        Ok((registry, type_def))
    }

    fn transcode_roundtrip<T>(input: &str, expected_output: Value) -> Result<()>
    where
        T: scale_info::TypeInfo + 'static,
    {
        let (registry, ty) = registry_with_type::<T>()?;

        let value = ron::from_str(input).context("Invalid RON value")?;
        let mut output = Vec::new();
        ty.encode_value_to(&registry, &value, &mut output)?;
        // println!("transcode_roundtrip: {:?}", output);
        let decoded = ty.decode_value(&registry, &mut &output[..])?;
        assert_eq!(expected_output, decoded);
        Ok(())
    }

    #[test]
    fn transcode_bool() -> Result<()> {
        transcode_roundtrip::<bool>("true", Value::Bool(true))?;
        transcode_roundtrip::<bool>("false", Value::Bool(false))
    }

    #[test]
    fn transcode_char_unsupported() -> Result<()> {
        let (registry, ty) = registry_with_type::<char>()?;

        let encoded = u32::from('c').encode();

        assert!(ty
            .encode_value_to(&registry, &Value::Char('c'), &mut Vec::new())
            .is_err());
        assert!(ty.decode_value(&registry, &mut &encoded[..]).is_err());
        Ok(())
    }

    #[test]
    fn transcode_str() -> Result<()> {
        transcode_roundtrip::<String>("\"ink!\"", Value::String("ink!".to_string()))
    }

    #[test]
    fn transcode_unsigned_integers() -> Result<()> {
        transcode_roundtrip::<u8>("0", Value::Number(ron::Number::Integer(0)))?;
        transcode_roundtrip::<u8>("255", Value::Number(ron::Number::Integer(255)))?;

        transcode_roundtrip::<u16>("0", Value::Number(ron::Number::Integer(0)))?;
        transcode_roundtrip::<u16>("65535", Value::Number(ron::Number::Integer(65535)))?;

        transcode_roundtrip::<u32>("0", Value::Number(ron::Number::Integer(0)))?;
        transcode_roundtrip::<u32>(
            "4294967295",
            Value::Number(ron::Number::Integer(4294967295)),
        )?;

        transcode_roundtrip::<u64>("0", Value::Number(ron::Number::Integer(0)))?;
        transcode_roundtrip::<u64>(
            "\"18_446_744_073_709_551_615\"",
            Value::String("18446744073709551615".to_string()),
        )?;

        transcode_roundtrip::<u128>("0", Value::Number(ron::Number::Integer(0)))?;
        transcode_roundtrip::<u128>(
            "\"340_282_366_920_938_463_463_374_607_431_768_211_455\"",
            Value::String("340282366920938463463374607431768211455".to_string()),
        )
    }

    #[test]
    #[ignore]
    fn transcode_integers() -> Result<()> {
        todo!()
    }

    #[test]
    fn transcode_byte_array() -> Result<()> {
        transcode_roundtrip::<[u8; 2]>("\"0000\"", Value::String("0000".to_string()))?;
        transcode_roundtrip::<[u8; 4]>("\"0xDEADBEEF\"", Value::String("deadbeef".to_string()))?;
        transcode_roundtrip::<[u8; 4]>("\"0xdeadbeef\"", Value::String("deadbeef".to_string()))
    }

    #[test]
    fn transcode_array() -> Result<()> {
        transcode_roundtrip::<[u32; 3]>(
            "[1, 2, 3]",
            Value::Seq(vec![
                Value::Number(Number::Integer(1)),
                Value::Number(Number::Integer(2)),
                Value::Number(Number::Integer(3)),
            ]),
        )?;
        transcode_roundtrip::<[String; 2]>(
            "[\"hello\", \"world\"]",
            Value::Seq(vec![
                Value::String("hello".to_string()),
                Value::String("world".to_string()),
            ]),
        )
    }

    #[test]
    fn transcode_seq() -> Result<()> {
        transcode_roundtrip::<Vec<u32>>(
            "[1, 2, 3]",
            Value::Seq(vec![
                Value::Number(Number::Integer(1)),
                Value::Number(Number::Integer(2)),
                Value::Number(Number::Integer(3)),
            ]),
        )?;
        transcode_roundtrip::<Vec<String>>(
            "[\"hello\", \"world\"]",
            Value::Seq(vec![
                Value::String("hello".to_string()),
                Value::String("world".to_string()),
            ]),
        )
    }

    #[test]
    #[ignore]
    fn transcode_tuple() -> Result<()> {
        todo!()
    }

    #[test]
    fn transcode_composite() -> Result<()> {
        #[allow(dead_code)]
        #[derive(TypeInfo)]
        struct S {
            a: u32,
            b: String,
            c: [u8; 4],
            // nested struct
            d: Vec<S>,
        }

        transcode_roundtrip::<S>(
            r#"S(a: 1, b: "ink!", c: "0xDEADBEEF", d: [S(a: 2, b: "ink!", c: "0xDEADBEEF", d: [])])"#,
            Value::Map(
                vec![
                    (
                        Value::String("a".to_string()),
                        Value::Number(Number::Integer(1)),
                    ),
                    (
                        Value::String("b".to_string()),
                        Value::String("ink!".to_string()),
                    ),
                    (
                        Value::String("c".to_string()),
                        Value::String("deadbeef".to_string()),
                    ),
                    (
                        Value::String("d".to_string()),
                        Value::Seq(vec![Value::Map(
                            vec![
                                (
                                    Value::String("a".to_string()),
                                    Value::Number(Number::Integer(2)),
                                ),
                                (
                                    Value::String("b".to_string()),
                                    Value::String("ink!".to_string()),
                                ),
                                (
                                    Value::String("c".to_string()),
                                    Value::String("deadbeef".to_string()),
                                ),
                                (
                                    Value::String("d".to_string()),
                                    Value::Seq(Vec::new().into_iter().collect()),
                                ),
                            ]
                            .into_iter()
                            .collect(),
                        )]),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        )
    }

    #[test]
    fn transcode_variant() -> Result<()> {
        #[derive(TypeInfo)]
        #[allow(dead_code)]
        enum E {
            A,
            B(u32, String),
            C { a: [u8; 4], b: Vec<E> },
        }

        let v: ron::Value = ron::from_str(r#"A(1, "two")"#)?;
        assert_eq!(ron::Value::Unit, v);

        Ok(())

        // transcode_roundtrip::<E>(
        //     r#"A()"#,
        //     // the RON/serde data model does not support enum variants, so we have to make it a Map
        //     // with the key being the variant name
        //     Value::Map(vec![
        //         (
        //             Value::String("A".to_string()),
        //             Value::Unit
        //         )
        //     ].into_iter().collect())
        // )
    }

    #[test]
    #[ignore]
    fn transcode_option() -> Result<()> {
        todo!()
    }
}

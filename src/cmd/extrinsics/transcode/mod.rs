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
mod scon;

use self::{
    decode::decode_value,
    encode::encode_value,
    scon::{Map, Value},
};

use anyhow::Result;
use ink_metadata::{ConstructorSpec, InkProject, MessageSpec};
use scale::Input;
use scale_info::{
    form::{CompactForm, Form},
    Field, RegistryReadOnly, TypeDefComposite,
};
use std::fmt::{self, Debug, Display, Formatter};

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
        S: AsRef<str> + Debug,
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
            let value = scon::from_str(arg.as_ref())?;
            encode_value(self.registry(), spec.ty().ty().id(), &value, &mut encoded)?;
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

    pub fn decode_contract_event<I>(&self, data: &mut I) -> Result<ContractEvent>
    where
        I: Input + Debug,
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
            let name = arg.name().to_string();
            let value = decode_value(self.registry(), arg.ty().ty().id(), data)?;
            args.push((Value::String(name), value));
        }

        let name = event_spec.name().to_string();
        let map = Map::new(Some(&name), args.into_iter().collect());

        Ok(ContractEvent { name, value: Value::Map(map) })
    }

    pub fn decode_return(&self, name: &str, data: Vec<u8>) -> Result<Value> {
        let msg_spec = self.find_message_spec(name).ok_or(anyhow::anyhow!(
            "Failed to find message spec with name '{}'",
            name
        ))?;
        if let Some(return_ty) = msg_spec.return_type().opt_type() {
            decode_value(self.registry(), return_ty.ty().id(), &mut &data[..])
        } else {
            Ok(Value::Unit)
        }
    }
}

#[derive(Debug)]
pub enum CompositeTypeFields {
    StructNamedFields(Vec<CompositeTypeNamedField>),
    TupleStructUnnamedFields(Vec<Field<CompactForm>>),
    NoFields,
}

#[derive(Debug)]
pub struct CompositeTypeNamedField {
    name: <CompactForm as Form>::String,
    field: Field<CompactForm>,
}

impl CompositeTypeNamedField {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn field(&self) -> &Field<CompactForm> {
        &self.field
    }
}

impl CompositeTypeFields {
    pub fn from_type_def(type_def: &TypeDefComposite<CompactForm>) -> Result<Self> {
        if type_def.fields().is_empty() {
            Ok(Self::NoFields)
        } else if type_def.fields().iter().all(|f| f.name().is_some()) {
            let fields = type_def
                .fields()
                .iter()
                .map(|f| CompositeTypeNamedField {
                    name: f.name().expect("All fields have a name; qed").to_owned(),
                    field: f.clone(),
                })
                .collect();
            Ok(Self::StructNamedFields(fields))
        } else if type_def.fields().iter().all(|f| f.name().is_none()) {
            Ok(Self::TupleStructUnnamedFields(type_def.fields().to_vec()))
        } else {
            Err(anyhow::anyhow!(
                "Struct fields should either be all named or all unnamed"
            ))
        }
    }
}

pub struct ContractEvent {
    pub name: String,
    pub value: Value,
}

impl Display for ContractEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        <Value as Display>::fmt(&self.value, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use scale::Encode;
    use scale_info::{MetaType, Registry, TypeInfo};
    use scon::{Tuple, Value};
    use std::num::NonZeroU32;

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

    fn registry_with_type<T>() -> Result<(RegistryReadOnly, NonZeroU32)>
    where
        T: scale_info::TypeInfo + 'static,
    {
        let mut registry = Registry::new();
        let type_id = registry.register_type(&MetaType::new::<T>());
        let registry: RegistryReadOnly = registry.into();

        Ok((registry, type_id.id()))
    }

    fn transcode_roundtrip<T>(input: &str, expected_output: Value) -> Result<()>
    where
        T: scale_info::TypeInfo + 'static,
    {
        let (registry, ty) = registry_with_type::<T>()?;

        let value = scon::from_str(input).context("Invalid SON value")?;
        let mut output = Vec::new();
        encode_value(&registry, ty, &value, &mut output)?;
        let decoded = decode_value(&registry, ty, &mut &output[..])?;
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

        assert!(encode_value(&registry, ty, &Value::Char('c'), &mut Vec::new()).is_err());
        assert!(decode_value(&registry, ty, &mut &encoded[..]).is_err());
        Ok(())
    }

    #[test]
    fn transcode_str() -> Result<()> {
        transcode_roundtrip::<String>("\"ink!\"", Value::String("ink!".to_string()))
    }

    #[test]
    fn transcode_unsigned_integers() -> Result<()> {
        transcode_roundtrip::<u8>("0", Value::UInt(0))?;
        transcode_roundtrip::<u8>("255", Value::UInt(255))?;

        transcode_roundtrip::<u16>("0", Value::UInt(0))?;
        transcode_roundtrip::<u16>("65535", Value::UInt(65535))?;

        transcode_roundtrip::<u32>("0", Value::UInt(0))?;
        transcode_roundtrip::<u32>("4294967295", Value::UInt(4294967295))?;

        transcode_roundtrip::<u64>("0", Value::UInt(0))?;
        transcode_roundtrip::<u64>(
            "\"18_446_744_073_709_551_615\"",
            Value::UInt(18446744073709551615),
        )?;

        transcode_roundtrip::<u128>("0", Value::UInt(0))?;
        transcode_roundtrip::<u128>(
            "\"340_282_366_920_938_463_463_374_607_431_768_211_455\"",
            Value::UInt(340282366920938463463374607431768211455),
        )
    }

    #[test]
    fn transcode_integers() -> Result<()> {
        transcode_roundtrip::<i8>("-128", Value::Int(i8::min_value().into()))?;
        transcode_roundtrip::<i8>("127", Value::Int(i8::max_value().into()))?;

        transcode_roundtrip::<i16>("-32768", Value::Int(i16::min_value().into()))?;
        transcode_roundtrip::<i16>("32767", Value::Int(i16::max_value().into()))?;

        transcode_roundtrip::<i32>("-2147483648", Value::Int(i32::min_value().into()))?;
        transcode_roundtrip::<i32>("2147483647", Value::Int(i32::max_value().into()))?;

        transcode_roundtrip::<i64>("-9223372036854775808", Value::Int(i64::min_value().into()))?;
        transcode_roundtrip::<i64>(
            "\"9_223_372_036_854_775_807\"",
            Value::Int(i64::max_value().into()),
        )?;

        transcode_roundtrip::<i128>(
            "-170141183460469231731687303715884105728",
            Value::Int(i128::min_value()),
        )?;
        transcode_roundtrip::<i128>(
            "\"170141183460469231731687303715884105727\"",
            Value::Int(i128::max_value()),
        )
    }

    #[test]
    fn transcode_byte_array() -> Result<()> {
        transcode_roundtrip::<[u8; 2]>(r#"0x0000"#, Value::Bytes(vec![0x00, 0x00].into()))?;
        transcode_roundtrip::<[u8; 4]>(
            r#"0xDEADBEEF"#,
            Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF].into()),
        )?;
        transcode_roundtrip::<[u8; 4]>(
            r#"0xdeadbeef"#,
            Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF].into()),
        )
    }

    #[test]
    fn transcode_array() -> Result<()> {
        transcode_roundtrip::<[u32; 3]>(
            "[1, 2, 3]",
            Value::Seq(vec![Value::UInt(1), Value::UInt(2), Value::UInt(3)].into()),
        )?;
        transcode_roundtrip::<[String; 2]>(
            "[\"hello\", \"world\"]",
            Value::Seq(
                vec![
                    Value::String("hello".to_string()),
                    Value::String("world".to_string()),
                ]
                .into(),
            ),
        )
    }

    #[test]
    fn transcode_seq() -> Result<()> {
        transcode_roundtrip::<Vec<u32>>(
            "[1, 2, 3]",
            Value::Seq(vec![Value::UInt(1), Value::UInt(2), Value::UInt(3)].into()),
        )?;
        transcode_roundtrip::<Vec<String>>(
            "[\"hello\", \"world\"]",
            Value::Seq(
                vec![
                    Value::String("hello".to_string()),
                    Value::String("world".to_string()),
                ]
                .into(),
            ),
        )
    }

    #[test]
    fn transcode_tuple() -> Result<()> {
        transcode_roundtrip::<(u32, String, [u8; 4])>(
            r#"(1, "ink!", 0xDEADBEEF)"#,
            Value::Tuple(Tuple::new(
                None,
                vec![
                    Value::UInt(1),
                    Value::String("ink!".to_string()),
                    Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF].into()),
                ],
            )),
        )
    }

    #[test]
    fn transcode_composite_struct() -> Result<()> {
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
            r#"S(a: 1, b: "ink!", c: 0xDEADBEEF, d: [S(a: 2, b: "ink!", c: 0xDEADBEEF, d: [])])"#,
            Value::Map(
                vec![
                    (Value::String("a".to_string()), Value::UInt(1)),
                    (
                        Value::String("b".to_string()),
                        Value::String("ink!".to_string()),
                    ),
                    (
                        Value::String("c".to_string()),
                        Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF].into()),
                    ),
                    (
                        Value::String("d".to_string()),
                        Value::Seq(
                            vec![Value::Map(
                                vec![
                                    (Value::String("a".to_string()), Value::UInt(2)),
                                    (
                                        Value::String("b".to_string()),
                                        Value::String("ink!".to_string()),
                                    ),
                                    (
                                        Value::String("c".to_string()),
                                        Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF].into()),
                                    ),
                                    (
                                        Value::String("d".to_string()),
                                        Value::Seq(
                                            Vec::new().into_iter().collect::<Vec<_>>().into(),
                                        ),
                                    ),
                                ]
                                .into_iter()
                                .collect(),
                            )]
                            .into(),
                        ),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        )
    }

    #[test]
    fn transcode_composite_tuple_struct() -> Result<()> {
        #[allow(dead_code)]
        #[derive(TypeInfo)]
        struct S(u32, String, [u8; 4]);

        transcode_roundtrip::<S>(
            r#"S(1, "ink!", 0xDEADBEEF)"#,
            Value::Tuple(Tuple::new(
                Some("S"),
                vec![
                    Value::UInt(1),
                    Value::String("ink!".to_string()),
                    Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF].into()),
                ],
            )),
        )
    }

    #[test]
    fn transcode_composite_single_field_struct() -> Result<()> {
        #[allow(dead_code)]
        #[derive(TypeInfo)]
        struct S([u8; 4]);

        transcode_roundtrip::<S>(
            r#"0xDEADBEEF"#,
            Value::Tuple(Tuple::new(
                Some("S"),
                vec![Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF].into())].into(),
            )),
        )
    }

    #[test]
    fn transcode_composite_single_field_tuple() -> Result<()> {
        env_logger::try_init()?;
        transcode_roundtrip::<([u8; 4],)>(
            r#"0xDEADBEEF"#,
            Value::Tuple(Tuple::new(
                None,
                vec![Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF].into())].into(),
            )),
        )
    }

    #[test]
    fn transcode_variant() -> Result<()> {
        #[derive(TypeInfo)]
        #[allow(dead_code)]
        enum E {
            A(u32, String),
            B { a: [u8; 4], b: Vec<E> },
            C,
        }

        transcode_roundtrip::<E>(
            r#"A(1, "2")"#,
            Value::Tuple(Tuple::new(
                Some("A"),
                vec![Value::UInt(1), Value::String("2".into())],
            )),
        )
    }

    #[test]
    fn transcode_option() -> Result<()> {
        transcode_roundtrip::<Option<u32>>(
            r#"Some(32)"#,
            Value::Tuple(Tuple::new(Some("Some"), vec![Value::UInt(32).into()])),
        )?;

        transcode_roundtrip::<Option<u32>>(
            r#"None"#,
            Value::Tuple(Tuple::new(Some("None"), Vec::new())),
        )
    }
}

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
    decode::Decoder,
    encode::Encoder,
    env_types::{EnvTypesTranscoder, TypeLookupId},
    scon::{Map, Value},
};

use anyhow::Result;
use scale::Output;
use scale_info::RegistryReadOnly;
use std::fmt::Debug;

/// Encode strings to SCALE encoded output.
/// Decode SCALE encoded input into `Value` objects.
pub struct Transcoder<'a> {
    registry: &'a RegistryReadOnly,
    env_types: EnvTypesTranscoder,
}

impl<'a> Transcoder<'a> {
    pub fn new(registry: &'a RegistryReadOnly) -> Self {
        Self {
            registry,
            env_types: EnvTypesTranscoder::new(registry),
        }
    }

    pub fn encode<T, O>(&self, ty: T, value: &Value, output: &mut O) -> Result<()>
    where
        T: Into<TypeLookupId>,
        O: Output + Debug,
    {
        let encoder = Encoder::new(self.registry, &self.env_types);
        encoder.encode(ty, &value, output)
    }

    pub fn decode<T>(&self, ty: T, input: &mut &[u8]) -> Result<Value>
    where
        T: Into<TypeLookupId>,
    {
        let decoder = Decoder::new(self.registry, &self.env_types);
        decoder.decode(ty, input)
    }
}

#[cfg(test)]
mod tests {
    use super::super::scon::{Tuple, Value};
    use super::*;
    use anyhow::Context;
    use scale::Encode;
    use scale_info::{MetaType, Registry, TypeInfo};
    use std::num::NonZeroU32;

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
        let transcoder = Transcoder::new(&registry);

        let value = input.parse::<Value>().context("Invalid SON value")?;
        let mut output = Vec::new();
        transcoder.encode(ty, &value, &mut output)?;
        let decoded = transcoder.decode(ty, &mut &output[..])?;
        assert_eq!(expected_output, decoded, "decoding");
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
        let transcoder = Transcoder::new(&registry);
        let encoded = u32::from('c').encode();

        assert!(transcoder
            .encode(ty, &Value::Char('c'), &mut Vec::new())
            .is_err());
        assert!(transcoder.decode(ty, &mut &encoded[..]).is_err());
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

    #[test]
    fn transcode_account_id_custom_ss58_encoding() -> Result<()> {
        env_logger::init();

        type AccountId = ink_env::AccountId;

        #[allow(dead_code)]
        #[derive(TypeInfo)]
        struct S {
            no_alias: ink_env::AccountId,
            aliased: AccountId,
        }

        transcode_roundtrip::<S>(
            r#"S(
                no_alias: 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY,
                aliased: 5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty,
             )"#,
            Value::Map(Map::new(
                Some("S"),
                vec![
                    (
                        Value::String("no_alias".into()),
                        Value::Literal("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY".into()),
                    ),
                    (
                        Value::String("aliased".into()),
                        Value::Literal("5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty".into()),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
        )
    }

    #[test]
    fn transcode_account_id_custom_encoding_fails_with_non_matching_type_name() -> Result<()> {
        type SomeOtherAlias = ink_env::AccountId;

        #[allow(dead_code)]
        #[derive(TypeInfo)]
        struct S {
            aliased: SomeOtherAlias,
        }

        let (registry, ty) = registry_with_type::<S>()?;
        let transcoder = Transcoder::new(&registry);

        let input = r#"S( aliased: 5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty )"#;
        let value = input.parse::<Value>()?;
        let mut output = Vec::new();
        assert!(transcoder.encode(ty, &value, &mut output).is_err());

        Ok(())
    }
}

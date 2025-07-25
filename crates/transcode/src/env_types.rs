// Copyright (C) Use Ink (UK) Ltd.
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

use crate::{
    assert_not_shortened_hex,
    AccountId32,
    Hex,
    Value,
};
use anyhow::{
    Context,
    Result,
};
use primitive_types::U128;
use scale::{
    Decode,
    Encode,
    Output,
};
use scale_info::{
    form::PortableForm,
    IntoPortable,
    Path,
    TypeInfo,
};
use std::{
    boxed::Box,
    collections::HashMap,
    convert::TryFrom,
    str::FromStr,
};

/// Provides custom encoding and decoding for predefined environment types.
#[derive(Default)]
pub struct EnvTypesTranscoder {
    encoders: HashMap<u32, Box<dyn CustomTypeEncoder>>,
    decoders: HashMap<u32, Box<dyn CustomTypeDecoder>>,
}

impl EnvTypesTranscoder {
    /// Construct an `EnvTypesTranscoder` from the given type registry.
    pub fn new(
        encoders: HashMap<u32, Box<dyn CustomTypeEncoder>>,
        decoders: HashMap<u32, Box<dyn CustomTypeDecoder>>,
    ) -> Self {
        Self { encoders, decoders }
    }

    /// If the given type id is for a type with custom encoding, encodes the given value
    /// with the custom encoder and returns `true`. Otherwise returns `false`.
    ///
    /// # Errors
    ///
    /// - If the custom encoding fails.
    pub fn try_encode<O>(
        &self,
        type_id: u32,
        value: &Value,
        output: &mut O,
    ) -> Result<bool>
    where
        O: Output,
    {
        match self.encoders.get(&type_id) {
            Some(encoder) => {
                tracing::debug!("Encoding type {:?} with custom encoder", type_id);
                let encoded_env_type = encoder
                    .encode_value(value)
                    .context("Error encoding custom type")?;
                output.write(&encoded_env_type);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// If the given type lookup id is for an environment type with custom
    /// decoding, decodes the given input with the custom decoder and returns
    /// `Some(value)`. Otherwise returns `None`.
    ///
    /// # Errors
    ///
    /// - If the custom decoding fails.
    pub fn try_decode(&self, type_id: u32, input: &mut &[u8]) -> Result<Option<Value>> {
        match self.decoders.get(&type_id) {
            Some(decoder) => {
                tracing::debug!("Decoding type {:?} with custom decoder", type_id);
                let decoded = decoder.decode_value(input)?;
                Ok(Some(decoded))
            }
            None => {
                tracing::debug!("No custom decoder found for type {:?}", type_id);
                Ok(None)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct PathKey(Vec<String>);

impl PathKey {
    pub fn from_type<T>() -> Self
    where
        T: TypeInfo,
    {
        let type_info = T::type_info();
        let path = type_info.path.into_portable(&mut Default::default());
        PathKey::from(&path)
    }
}

impl From<&Path<PortableForm>> for PathKey {
    fn from(path: &Path<PortableForm>) -> Self {
        PathKey(path.segments.to_vec())
    }
}

pub type TypesByPath = HashMap<PathKey, u32>;

/// Implement this trait to define custom encoding for a type in a `scale-info` type
/// registry.
pub trait CustomTypeEncoder: Send + Sync {
    fn encode_value(&self, value: &Value) -> Result<Vec<u8>>;
}

/// Implement this trait to define custom decoding for a type in a `scale-info` type
/// registry.
pub trait CustomTypeDecoder: Send + Sync {
    fn decode_value(&self, input: &mut &[u8]) -> Result<Value>;
}

/// Custom encoding/decoding for the Substrate `AccountId` type.
///
/// Enables an `AccountId` to be input/output as an SS58 Encoded literal e.g.
/// `5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY`.
#[derive(Clone)]
pub struct AccountId;

impl CustomTypeEncoder for AccountId {
    fn encode_value(&self, value: &Value) -> Result<Vec<u8>> {
        let account_id = match value {
            Value::Literal(literal) => {
                AccountId32::from_str(literal).map_err(|e| {
                    anyhow::anyhow!(
                        "Error parsing AccountId from literal `{}`: {}",
                        literal,
                        e
                    )
                })?
            }
            Value::String(string) => {
                AccountId32::from_str(string).map_err(|e| {
                    anyhow::anyhow!(
                        "Error parsing AccountId from string '{}': {}",
                        string,
                        e
                    )
                })?
            }
            Value::Hex(hex) => {
                AccountId32::try_from(hex.bytes()).map_err(|_| {
                    anyhow::anyhow!(
                        "Error converting hex bytes `{:?}` to AccountId",
                        hex.bytes()
                    )
                })?
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Expected a string, literal, or hex for an AccountId"
                ))
            }
        };
        Ok(account_id.encode())
    }
}

impl CustomTypeDecoder for AccountId {
    fn decode_value(&self, input: &mut &[u8]) -> Result<Value> {
        let account_id = AccountId32::decode(input)?;
        Ok(Value::Literal(account_id.to_ss58check()))
    }
}

/// Custom decoding for the `Hash` or `[u8; 32]` type so that it is displayed as a hex
/// encoded string.
#[derive(Clone)]
pub struct Hash;

impl CustomTypeEncoder for Hash {
    fn encode_value(&self, value: &Value) -> Result<Vec<u8>> {
        // todo currently using H256 here
        let h256 = match value {
            Value::Literal(literal) => {
                primitive_types::H256::from_str(literal).map_err(|e| {
                    anyhow::anyhow!(
                        "Error parsing H256 from literal `{}`: {}",
                        literal,
                        e
                    )
                })?
            }
            Value::String(string) => {
                primitive_types::H256::from_str(string).map_err(|e| {
                    anyhow::anyhow!("Error parsing H256 from string '{}': {}", string, e)
                })?
            }
            Value::Hex(hex) => primitive_types::H256::from_slice(hex.bytes()),
            _ => {
                return Err(anyhow::anyhow!(
                    "Expected a string, hex, uint, or literal for a U256"
                ))
            }
        };
        Ok(h256.encode())
    }
}

impl CustomTypeDecoder for Hash {
    fn decode_value(&self, input: &mut &[u8]) -> Result<Value> {
        let hash = primitive_types::H256::decode(input)?;
        Ok(Value::Hex(Hex::from_str(&format!("{hash:?}"))?))
    }
}

/// Custom decoding for the `H160` or `[u8; 20]` type so that it is displayed as a hex
/// encoded string.
#[derive(Clone)]
pub struct H160;

impl CustomTypeDecoder for H160 {
    fn decode_value(&self, input: &mut &[u8]) -> Result<Value> {
        let h160 = primitive_types::H160::decode(input)?;
        Ok(Value::Hex(Hex::from_str(&format!("{h160:?}"))?))
    }
}

impl CustomTypeEncoder for H160 {
    fn encode_value(&self, value: &Value) -> Result<Vec<u8>> {
        let h160 = match value {
            Value::Literal(literal) => {
                primitive_types::H160::from_str(literal).map_err(|e| {
                    anyhow::anyhow!(
                        "Error parsing H160 from literal `{}`: {}",
                        literal,
                        e
                    )
                })?
            }
            Value::String(string) => {
                assert_not_shortened_hex(string);
                primitive_types::H160::from_str(string).map_err(|e| {
                    anyhow::anyhow!("Error parsing H160 from string '{}': {}", string, e)
                })?
            }
            Value::Hex(hex) => primitive_types::H160::from_slice(hex.bytes()),
            _ => {
                return Err(anyhow::anyhow!(
                    "Expected a string, literal, or hex for an H160"
                ))
            }
        };
        Ok(h160.encode())
    }
}

/// Custom decoding for the `U256` or `[u8; 32]` type so that it is displayed as a hex
/// encoded string.
#[derive(Clone)]
pub struct U256;

impl CustomTypeDecoder for U256 {
    fn decode_value(&self, input: &mut &[u8]) -> Result<Value> {
        let u256 = primitive_types::U256::decode(input)?;
        Ok(Value::Literal(format!("{u256}")))
    }
}

impl CustomTypeEncoder for U256 {
    fn encode_value(&self, value: &Value) -> Result<Vec<u8>> {
        let u256 = match value {
            Value::Literal(literal) => {
                primitive_types::U256::from_str(literal).map_err(|e| {
                    anyhow::anyhow!(
                        "Error parsing U256 from literal `{}`: {}",
                        literal,
                        e
                    )
                })?
            }
            Value::String(string) => {
                primitive_types::U256::from_str(string).map_err(|e| {
                    anyhow::anyhow!("Error parsing U256 from string '{}': {}", string, e)
                })?
            }
            Value::UInt(uint128) => {
                let u_128 = U128::from(*uint128);
                primitive_types::U256::from(u_128)
            }
            // todo from_slice?
            Value::Hex(hex) => primitive_types::U256::from_little_endian(hex.bytes()),
            _ => {
                return Err(anyhow::anyhow!(
                    "Expected a string, hex, uint, or literal for a U256"
                ))
            }
        };
        let ret = u256.encode();
        Ok(ret)
    }
}

/// Custom decoding for the `H256` or `[u8; 32]` type so that it is displayed as a hex
/// encoded string.
#[derive(Clone)]
pub struct H256;

impl CustomTypeDecoder for H256 {
    fn decode_value(&self, input: &mut &[u8]) -> Result<Value> {
        let h256 = primitive_types::H256::decode(input)?;
        Ok(Value::Hex(Hex::from_str(&format!("{h256:?}"))?))
    }
}

impl CustomTypeEncoder for H256 {
    fn encode_value(&self, value: &Value) -> Result<Vec<u8>> {
        let h256 = match value {
            Value::Literal(literal) => {
                primitive_types::H256::from_str(literal).map_err(|e| {
                    anyhow::anyhow!(
                        "Error parsing H256 from literal `{}`: {}",
                        literal,
                        e
                    )
                })?
            }
            Value::String(string) => {
                primitive_types::H256::from_str(string).map_err(|e| {
                    anyhow::anyhow!("Error parsing H256 from string '{}': {}", string, e)
                })?
            }
            Value::Hex(hex) => primitive_types::H256::from_slice(hex.bytes()),
            _ => {
                return Err(anyhow::anyhow!(
                    "Expected a string, hex, uint, or literal for a H256"
                ))
            }
        };
        Ok(h256.encode())
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;
}
*/

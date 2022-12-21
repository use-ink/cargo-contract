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

use super::scon::Value;
use crate::scon::Hex;
use anyhow::{
    Context,
    Result,
};
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
use sp_core::crypto::{
    AccountId32,
    Ss58Codec,
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
    encoders: HashMap<u32, Box<dyn CustomTypeEncoder + Send + Sync>>,
    decoders: HashMap<u32, Box<dyn CustomTypeDecoder + Send + Sync>>,
}

impl EnvTypesTranscoder {
    /// Construct an `EnvTypesTranscoder` from the given type registry.
    pub fn new(
        encoders: HashMap<u32, Box<dyn CustomTypeEncoder + Send + Sync>>,
        decoders: HashMap<u32, Box<dyn CustomTypeDecoder + Send + Sync>>,
    ) -> Self {
        Self { encoders, decoders }
    }

    /// If the given type id is for a type with custom encoding, encodes the given value with the
    /// custom encoder and returns `true`. Otherwise returns `false`.
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
        let path = type_info
            .path()
            .clone()
            .into_portable(&mut Default::default());
        PathKey::from(&path)
    }
}

impl From<&Path<PortableForm>> for PathKey {
    fn from(path: &Path<PortableForm>) -> Self {
        PathKey(path.segments().to_vec())
    }
}

pub type TypesByPath = HashMap<PathKey, u32>;

/// Implement this trait to define custom encoding for a type in a `scale-info` type registry.
pub trait CustomTypeEncoder {
    fn encode_value(&self, value: &Value) -> Result<Vec<u8>>;
}

/// Implement this trait to define custom decoding for a type in a `scale-info` type registry.
pub trait CustomTypeDecoder {
    fn decode_value(&self, input: &mut &[u8]) -> Result<Value>;
}

/// Custom encoding/decoding for the Substrate `AccountId` type.
///
/// Enables an `AccountId` to be input/ouput as an SS58 Encoded literal e.g.
/// 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY
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
                    "Expected a string or a literal for an AccountId"
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

/// Custom decoding for the `Hash` or `[u8; 32]` type so that it is displayed as a hex encoded
/// string.
pub struct Hash;

impl CustomTypeDecoder for Hash {
    fn decode_value(&self, input: &mut &[u8]) -> Result<Value> {
        let hash = sp_core::H256::decode(input)?;
        Ok(Value::Hex(Hex::from_str(&format!("{:?}", hash))?))
    }
}

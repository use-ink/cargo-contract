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
use anyhow::Result;
use codec::{Decode, Encode, Output};
use ink_metadata::TypeSpec;
use scale_info::{form::PortableForm, Field, IntoPortable, Path, PortableRegistry, TypeInfo};
use sp_core::crypto::{AccountId32, Ss58Codec};
use std::{boxed::Box, collections::HashMap, convert::TryFrom, str::FromStr};

/// Provides custom encoding and decoding for predefined environment types.
pub struct EnvTypesTranscoder {
    transcoders: HashMap<TypeLookupId, Box<dyn CustomTypeTranscoder>>,
}

impl EnvTypesTranscoder {
    /// Construct an `EnvTypesTranscoder` from the given type registry.
    pub fn new(registry: &PortableRegistry) -> Self {
        let mut transcoders = HashMap::new();
        let types_by_path = registry
            .types()
            .iter()
            .map(|ty| (PathKey(ty.ty().path().segments().to_vec()), ty.id()))
            .collect::<TypesByPath>();
        log::debug!("Types by path: {:?}", types_by_path);
        Self::register_transcoder(&types_by_path, &mut transcoders, AccountId);
        Self::register_transcoder(&types_by_path, &mut transcoders, Balance);
        Self { transcoders }
    }

    fn register_transcoder<T>(
        type_lookup: &TypesByPath,
        transcoders: &mut HashMap<TypeLookupId, Box<dyn CustomTypeTranscoder>>,
        transcoder: T,
    ) where
        T: EnvType + CustomTypeTranscoder + 'static,
    {
        let type_id = TypeLookupId::from_env_type::<T>(type_lookup);

        if let Some(type_id) = type_id {
            let existing = transcoders.insert(type_id.clone(), Box::new(transcoder));
            log::debug!(
                "Registered environment type `{}` with id {:?}",
                T::ALIAS,
                type_id
            );
            if existing.is_some() {
                panic!(
                    "Attempted to register transcoder with existing type id {:?}",
                    type_id
                );
            }
        }
    }

    /// If the given `TypeLookupId`` is for an environment type with custom
    /// encoding, encodes the given value with the custom encoder and returns
    /// `true`. Otherwise returns `false`.
    ///
    /// # Errors
    ///
    /// - If the custom encoding fails.
    pub fn try_encode<O>(
        &self,
        type_id: &TypeLookupId,
        value: &Value,
        output: &mut O,
    ) -> Result<bool>
    where
        O: Output,
    {
        match self.transcoders.get(&type_id) {
            Some(transcoder) => {
                log::debug!("Encoding type {:?} with custom encoder", type_id);
                let encoded_env_type = transcoder.encode(value)?;
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
    pub fn try_decode(&self, type_id: &TypeLookupId, input: &mut &[u8]) -> Result<Option<Value>> {
        match self.transcoders.get(&type_id) {
            Some(transcoder) => {
                log::debug!("Decoding type {:?} with custom decoder", type_id);
                let decoded = transcoder.decode(input)?;
                Ok(Some(decoded))
            }
            None => Ok(None),
        }
    }
}

pub trait EnvType {
    type Type: TypeInfo;
    /// The name of the given environment type assigned by the `ink!` language macro.
    /// e.g. `Balance`, `AccountId` etc. are aliases to their underlying environment types.
    const ALIAS: &'static str;
}

/// Implement this trait to define custom transcoding for a type in a `scale-info` type registry.
trait CustomTypeTranscoder {
    fn encode(&self, value: &Value) -> Result<Vec<u8>>;
    fn decode(&self, input: &mut &[u8]) -> Result<Value>;
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct PathKey(Vec<String>);

impl From<Path<PortableForm>> for PathKey {
    fn from(path: Path<PortableForm>) -> Self {
        PathKey(path.segments().to_vec())
    }
}

type TypesByPath = HashMap<PathKey, u32>;

/// Unique identifier for a type used in a contract
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct TypeLookupId {
    /// The lookup id of the type in the `scale-info` type registry
    type_id: u32,
    /// The display name of the type, required to identify type aliases e.g. `type Balance = u128`
    maybe_alias: Option<String>,
}

impl TypeLookupId {
    /// Create a new `EnvTypeId` for the given `EnvType`, for the supplied type registry.
    ///
    /// Returns `None` if there is no matching type found in the registry. This is expected when the
    /// specified type is not used in a contract: it won't appear in the registry.
    ///
    pub fn from_env_type<T>(type_lookup: &TypesByPath) -> Option<Self>
    where
        T: EnvType,
    {
        let type_info = T::Type::type_info();
        let path = type_info
            .path()
            .clone()
            .into_portable(&mut Default::default());

        type_lookup.get(&path.into()).map(|type_id| Self {
            type_id: *type_id,
            maybe_alias: Some(T::ALIAS.to_owned()),
        })
    }

    /// Returns the type identifier for resolving the type from the registry.
    pub fn type_id(&self) -> u32 {
        self.type_id
    }
}

impl From<&TypeSpec<PortableForm>> for TypeLookupId {
    fn from(type_spec: &TypeSpec<PortableForm>) -> Self {
        Self {
            type_id: type_spec.ty().id(),
            maybe_alias: type_spec.display_name().segments().iter().last().cloned(),
        }
    }
}

impl From<&Field<PortableForm>> for TypeLookupId {
    fn from(field: &Field<PortableForm>) -> Self {
        Self {
            type_id: field.ty().id(),
            maybe_alias: field
                .type_name()
                .and_then(|n| n.split("::").last().map(ToOwned::to_owned)),
        }
    }
}

impl From<u32> for TypeLookupId {
    fn from(type_id: u32) -> Self {
        Self {
            type_id,
            maybe_alias: None,
        }
    }
}

struct AccountId;

impl EnvType for AccountId {
    type Type = <ink_env::DefaultEnvironment as ink_env::Environment>::AccountId;
    const ALIAS: &'static str = "AccountId";
}

impl CustomTypeTranscoder for AccountId {
    fn encode(&self, value: &Value) -> Result<Vec<u8>> {
        let account_id = match value {
            Value::Literal(literal) => AccountId32::from_str(literal).map_err(|e| {
                anyhow::anyhow!("Error parsing AccountId from literal `{}`: {}", literal, e)
            })?,
            Value::String(string) => AccountId32::from_str(string).map_err(|e| {
                anyhow::anyhow!("Error parsing AccountId from string '{}': {}", string, e)
            })?,
            Value::Bytes(bytes) => AccountId32::try_from(bytes.bytes()).map_err(|_| {
                anyhow::anyhow!("Error converting bytes `{:?}` to AccountId", bytes)
            })?,
            _ => Err(anyhow::anyhow!(
                "Expected a string or a literal for an AccountId"
            ))?,
        };
        Ok(account_id.encode())
    }

    fn decode(&self, input: &mut &[u8]) -> Result<Value> {
        let account_id = AccountId32::decode(input)?;
        Ok(Value::Literal(account_id.to_ss58check()))
    }
}

struct Balance;

impl EnvType for Balance {
    type Type = <ink_env::DefaultEnvironment as ink_env::Environment>::Balance;
    const ALIAS: &'static str = "Balance";
}

impl CustomTypeTranscoder for Balance {
    fn encode(&self, value: &Value) -> Result<Vec<u8>> {
        unimplemented!()
    }

    fn decode(&self, input: &mut &[u8]) -> Result<Value> {
        unimplemented!()
    }
}

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
use ink_metadata::TypeSpec;
use scale::{Encode, Output};
use scale_info::{
    form::CompactForm,
    RegistryReadOnly, TypeInfo, IntoCompact,
};
use sp_core::crypto::AccountId32;
use std::{
    boxed::Box,
    collections::HashMap,
    convert::TryFrom,
    num::NonZeroU32,
    str::FromStr,
};

/// Unique identifier for a type used in a contract
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
struct EnvTypeId {
    /// The lookup id of the type in the `scale-info` type registry
    type_id: NonZeroU32,
    /// The display name of the type, required to identify type aliases e.g. `type Balance = u128`
    display_name: Option<String>,
}

impl EnvTypeId {
    /// Create a new `EnvTypeId` for the given `EnvType`, for the supplied type registry.
    ///
    /// Returns `None` if there is no matching type found in the registry. This is expected when the
    /// specified type is not used in a contract: it won't appear in the registry.
    pub fn new<T>(registry: &RegistryReadOnly) -> Option<Self>
    where
        T: EnvType
    {
        // use this to extract all the types from the registry, todo: replace once `fn enumerate()` available in scale-info
        let mut enumerated_types = Vec::new(); //Vec<(NonZeroU32, &Type<CompactForm>>
        let mut i = 1;
        while let Some(ty) = registry.resolve(NonZeroU32::new(i).unwrap()) {
            enumerated_types.push((NonZeroU32::new(i).unwrap(), ty));
            i += 1;
        }

        let type_info = T::Type::type_info();
        let path = type_info
            .path()
            .clone()
            .into_compact(&mut Default::default());

        let type_id = enumerated_types
            .iter()
            .find_map(|(id, ty)| if ty.path() == &path { Some(id) } else { None });

        type_id.map(|type_id| {
            Self {
                type_id: *type_id,
                display_name: Some(T::ALIAS.to_owned()),
            }
        })
    }
}

impl From<&TypeSpec<CompactForm>> for EnvTypeId {
    fn from(type_spec: &TypeSpec<CompactForm>) -> Self {
        Self {
            type_id: type_spec.ty().id(),
            display_name: type_spec.display_name().segments().iter().last().cloned(),
        }
    }
}

pub struct EnvTypesTranscoder {
    encoders: HashMap<EnvTypeId, Box<dyn EnvTypeEncoder>>
}

impl EnvTypesTranscoder {
    pub fn new(registry: &RegistryReadOnly) -> Self {
        let mut transcoders = HashMap::new();
        Self::register_transcoder(registry, &mut transcoders, AccountId);
        Self::register_transcoder(registry, &mut transcoders, Balance);
        Self { encoders: transcoders }
    }

    fn register_transcoder<T>(registry: &RegistryReadOnly, transcoders: &mut HashMap<EnvTypeId, Box<dyn EnvTypeEncoder>>, transcoder: T)
    where
        T: EnvType + EnvTypeEncoder + 'static,
    {
        let type_id = EnvTypeId::new::<T>(registry);

        if let Some(type_id) = type_id {
            let existing = transcoders.insert(type_id.clone(), Box::new(transcoder));
            if existing.is_some() {
                panic!("Attempted to register transcoder with existing type id {:?}", type_id);
            }
        }
    }

    pub fn encode<O>(&self, type_spec: &TypeSpec<CompactForm>, value: &Value, output: &mut O) -> Result<bool>
    where
        O: Output
    {
        let type_id = type_spec.into();
        match self.encoders.get(&type_id) {
            Some(transcoder) => {
                let encoded_env_type = transcoder.encode(value)?;
                output.write(&encoded_env_type);
                Ok(true)
            }
            None => {
                Ok(false)
            }
        }
    }
}

pub trait EnvType {
    type Type: TypeInfo;
    /// The name of the given environment type assigned by the `ink!` language macro.
    /// e.g. `Balance`, `AccountId` etc. are aliases to their underlying environment types.
    const ALIAS: &'static str;
}

/// Implement this trait to define custom encoding for a type in a `scale-info` type registry.
pub trait EnvTypeEncoder {
    fn encode(&self, value: &Value) -> Result<Vec<u8>>;
}

struct AccountId;

impl EnvType for AccountId {
    type Type = <ink_env::DefaultEnvironment as ink_env::Environment>::AccountId;
    const ALIAS: &'static str = "AccountId";
}

impl EnvTypeEncoder for AccountId {
    fn encode(&self, value: &Value) -> Result<Vec<u8>> {
        let account_id =
            match value {
                Value::Literal(literal) => {
                    AccountId32::from_str(literal)
                        .map_err(|e| anyhow::anyhow!("Error parsing AccountId from literal `{}`: {}", literal, e))?
                }
                Value::String(string) => {
                    AccountId32::from_str(string)
                        .map_err(|e| anyhow::anyhow!("Error parsing AccountId from string '{}': {}", string, e))?
                },
                Value::Bytes(bytes) => {
                    AccountId32::try_from(bytes.bytes())
                        .map_err(|_| anyhow::anyhow!("Error converting bytes `{:?}` to AccountId", bytes))?
                },
                _ => Err(anyhow::anyhow!("Expected a string or a literal for an AccountId"))?
            };
        Ok(account_id.encode())
    }
}

struct Balance;

impl EnvType for Balance {
    type Type = <ink_env::DefaultEnvironment as ink_env::Environment>::Balance;
    const ALIAS: &'static str = "Balance";
}

impl EnvTypeEncoder for Balance {
    fn encode(&self, value: &Value) -> Result<Vec<u8>> {
        unimplemented!()
    }
}

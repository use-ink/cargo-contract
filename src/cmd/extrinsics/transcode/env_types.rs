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
    TypesByPath,
    TypeLookupId,
    scon::Value
};
use anyhow::Result;
use scale::{Encode, Output};
use scale_info::{RegistryReadOnly, TypeInfo};
use sp_core::crypto::AccountId32;
use std::{boxed::Box, collections::HashMap, convert::TryFrom, str::FromStr};

pub struct EnvTypesEncoder {
    encoders: HashMap<TypeLookupId, Box<dyn EnvTypeEncoder>>,
}

impl EnvTypesEncoder {
    pub fn new(registry: &RegistryReadOnly) -> Self {
        let mut transcoders = HashMap::new();
        let types_by_path = registry
            .enumerate()
            .map(|(id, ty)| (ty.path().clone().into(), id))
            .collect::<TypesByPath>();
        log::debug!("Types by path: {:?}", types_by_path);
        Self::register_transcoder(&types_by_path, &mut transcoders, AccountId);
        Self::register_transcoder(&types_by_path, &mut transcoders, Balance);
        Self {
            encoders: transcoders,
        }
    }

    fn register_transcoder<T>(
        type_lookup: &TypesByPath,
        transcoders: &mut HashMap<TypeLookupId, Box<dyn EnvTypeEncoder>>,
        transcoder: T,
    ) where
        T: EnvType + EnvTypeEncoder + 'static,
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

    /// If the given type spec is for an environment type with custom encoding, encodes the given
    /// value with the custom encoder and returns `true`. Otherwise returns `false`.
    pub fn try_encode<O>(
        &self,
        type_id: &TypeLookupId,
        value: &Value,
        output: &mut O,
    ) -> Result<bool>
    where
        O: Output,
    {
        match self.encoders.get(&type_id) {
            Some(transcoder) => {
                let encoded_env_type = transcoder.encode(value)?;
                output.write(&encoded_env_type);
                Ok(true)
            }
            None => Ok(false),
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

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
pub mod env_types;
mod scon;
mod transcoder;

pub use self::{
    scon::{Map, Value},
    transcoder::{Transcoder, TranscoderBuilder},
};

use anyhow::Result;
use ink_metadata::{ConstructorSpec, InkProject, MessageSpec};
use scale::Input;
use scale_info::{
    form::{Form, PortableForm},
    Field,
};
use std::fmt::Debug;

/// Encode strings to SCALE encoded smart contract calls.
/// Decode SCALE encoded smart contract events and return values into `Value` objects.
pub struct ContractMessageTranscoder<'a> {
    metadata: &'a InkProject,
    transcoder: Transcoder<'a>,
}

impl<'a> ContractMessageTranscoder<'a> {
    pub fn new(metadata: &'a InkProject) -> Self {
        let transcoder = TranscoderBuilder::new(metadata.registry())
            .register_custom_type::<<ink_env::DefaultEnvironment as ink_env::Environment>::AccountId, _>(env_types::AccountId)
            .done();
        Self {
            metadata,
            transcoder,
        }
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
            let value = scon::parse_value(arg.as_ref())?;
            self.transcoder
                .encode(spec.ty().ty().id(), &value, &mut encoded)?;
        }
        Ok(encoded)
    }

    fn constructors(&self) -> impl Iterator<Item = &ConstructorSpec<PortableForm>> {
        self.metadata.spec().constructors().iter()
    }

    fn messages(&self) -> impl Iterator<Item = &MessageSpec<PortableForm>> {
        self.metadata.spec().messages().iter()
    }

    fn find_message_spec(&self, name: &str) -> Option<&MessageSpec<PortableForm>> {
        self.messages()
            .find(|msg| msg.label().contains(&name.to_string()))
    }

    fn find_constructor_spec(&self, name: &str) -> Option<&ConstructorSpec<PortableForm>> {
        self.constructors()
            .find(|msg| msg.label().contains(&name.to_string()))
    }

    pub fn decode_contract_event(&self, data: &mut &[u8]) -> Result<Value> {
        let variant_index = data.read_byte()?;
        let event_spec = self
            .metadata
            .spec()
            .events()
            .get(variant_index as usize)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Event variant {} not found in contract metadata",
                    variant_index
                )
            })?;

        let mut args = Vec::new();
        for arg in event_spec.args() {
            let name = arg.label().to_string();
            let value = self.transcoder.decode(arg.ty().ty().id(), data)?;
            args.push((Value::String(name), value));
        }

        let name = event_spec.label().to_string();
        let map = Map::new(Some(&name), args.into_iter().collect());

        Ok(Value::Map(map))
    }

    pub fn decode_return(&self, name: &str, data: &mut &[u8]) -> Result<Value> {
        let msg_spec = self
            .find_message_spec(name)
            .ok_or_else(|| anyhow::anyhow!("Failed to find message spec with name '{}'", name))?;
        if let Some(return_ty) = msg_spec.return_type().opt_type() {
            self.transcoder.decode(return_ty.ty().id(), data)
        } else {
            Ok(Value::Unit)
        }
    }
}

#[derive(Debug)]
pub enum CompositeTypeFields {
    Named(Vec<CompositeTypeNamedField>),
    Unnamed(Vec<Field<PortableForm>>),
    NoFields,
}

#[derive(Debug)]
pub struct CompositeTypeNamedField {
    name: <PortableForm as Form>::String,
    field: Field<PortableForm>,
}

impl CompositeTypeNamedField {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn field(&self) -> &Field<PortableForm> {
        &self.field
    }
}

impl CompositeTypeFields {
    pub fn from_fields(fields: &[Field<PortableForm>]) -> Result<Self> {
        if fields.iter().next().is_none() {
            Ok(Self::NoFields)
        } else if fields.iter().all(|f| f.name().is_some()) {
            let fields = fields
                .iter()
                .map(|field| CompositeTypeNamedField {
                    name: field
                        .name()
                        .expect("All fields have a name; qed")
                        .to_owned(),
                    field: field.clone(),
                })
                .collect();
            Ok(Self::Named(fields))
        } else if fields.iter().all(|f| f.name().is_none()) {
            Ok(Self::Unnamed(fields.to_vec()))
        } else {
            Err(anyhow::anyhow!(
                "Struct fields should either be all named or all unnamed"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scale::Encode;
    use scon::Value;
    use std::str::FromStr;

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

            /// Dummy setter which receives the env type AccountId.
            #[ink(message)]
            pub fn set_account_id(&self, account_id: AccountId) {
                let _ = account_id;
            }
        }
    }

    fn generate_metadata() -> ink_metadata::InkProject {
        extern "Rust" {
            fn __ink_generate_metadata() -> ink_metadata::MetadataVersioned;
        }
        let metadata_versioned = unsafe { __ink_generate_metadata() };
        if let ink_metadata::MetadataVersioned::V3(ink_project) = metadata_versioned {
            ink_project
        } else {
            panic!("Expected metadata V3");
        }
    }

    #[test]
    fn encode_single_primitive_arg() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(&metadata);

        let encoded = transcoder.encode("new", &["true"])?;
        // encoded args follow the 4 byte selector
        let encoded_args = &encoded[4..];

        assert_eq!(true.encode(), encoded_args);
        Ok(())
    }

    #[test]
    fn encode_account_id_custom_ss58_encoding() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(&metadata);

        let encoded = transcoder.encode(
            "set_account_id",
            &["5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"],
        )?;

        // encoded args follow the 4 byte selector
        let encoded_args = &encoded[4..];

        let expected = sp_core::crypto::AccountId32::from_str(
            "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
        )
        .unwrap();
        assert_eq!(expected.encode(), encoded_args);
        Ok(())
    }

    #[test]
    fn decode_primitive_return() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(&metadata);

        let encoded = true.encode();
        let decoded = transcoder.decode_return("get", &mut &encoded[..])?;

        assert_eq!(Value::Bool(true), decoded);
        Ok(())
    }
}

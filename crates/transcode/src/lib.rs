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

//! For interacting with contracts from the command line, arguments need to be "transcoded" from
//! the string representation to the SCALE encoded representation.
//!
//! e.g. `"false" -> 0x00`
//!
//! And for displaying SCALE encoded data from events and RPC responses, it must be "transcoded"
//! in the other direction from the SCALE encoded representation to a human readable string.
//!
//! e.g. `0x00 -> "false"`
//!
//! Transcoding depends on [`scale-info`](https://github.com/paritytech/scale-info/) metadata in
//! order to dynamically determine the expected types.
//!
//! # Encoding
//!
//! First the string is parsed into an intermediate [`Value`]:
//!
//! `"false" -> Value::Bool(false)`
//!
//! This value is then matched with the metadata for the expected type in that context. e.g. the
//! [flipper](https://github.com/paritytech/ink/blob/master/examples/flipper/lib.rs) contract
//! accepts a `bool` argument to its `new` constructor, which will be reflected in the contract
//! metadata as [`scale_info::TypeDefPrimitive::Bool`].
//!
//! ```no_compile
//! #[ink(constructor)]
//! pub fn new(init_value: bool) -> Self {
//!     Self { value: init_value }
//! }
//! ```
//!
//! The parsed `Value::Bool(false)` argument value is then matched with the
//! [`scale_info::TypeDefPrimitive::Bool`] type metadata, and then the value can be safely encoded
//! as a `bool`, resulting in `0x00`, which can then be appended as data to the message to invoke
//! the constructor.
//!
//! # Decoding
//!
//! First the type of the SCALE encoded data is determined from the metadata. e.g. the return type
//! of a message when it is invoked as a "dry run" over RPC:
//!
//! ```no_compile
//! #[ink(message)]
//! pub fn get(&self) -> bool {
//!     self.value
//! }
//! ```
//!
//! The metadata will define the return type as [`scale_info::TypeDefPrimitive::Bool`], so that when
//! the raw data is received it can be decoded into the correct [`Value`], which is then converted
//! to a string for displaying to the user:
//!
//! `0x00 -> Value::Bool(false) -> "false"`
//!
//! # SCALE Object Notation (SCON)
//!
//! Complex types can be represented as strings using `SCON` for human-computer interaction. It is
//! intended to be similar to Rust syntax for instantiating types. e.g.
//!
//! `Foo { a: false, b: [0, 1, 2], c: "bar", d: (0, 1) }`
//!
//! This string could be parsed into a [`Value::Map`] and together with
//! [`scale_info::TypeDefComposite`] metadata could be transcoded into SCALE encoded bytes.
//!
//! As with the example for the primitive `bool` above, this works in the other direction for
//! decoding SCALE encoded bytes and converting them into a human readable string.
//!
//! # Example
//! ```no_run
//! # use contract_metadata::ContractMetadata;
//! # use contract_transcode::ContractMessageTranscoder;
//! # use std::{path::Path, fs::File};
//! let metadata_path = Path::new("/path/to/metadata.json");
//! let transcoder = ContractMessageTranscoder::load(metadata_path).unwrap();
//!
//! let constructor = "new";
//! let args = ["foo", "bar"];
//! let data = transcoder.encode(&constructor, &args).unwrap();
//!
//! println!("Encoded constructor data {:?}", data);
//! ```

mod decode;
mod encode;
pub mod env_types;
mod scon;
mod transcoder;
mod util;

pub use self::{
    scon::{
        Hex,
        Map,
        Tuple,
        Value,
    },
    transcoder::{
        Transcoder,
        TranscoderBuilder,
    },
};

use anyhow::{
    Context,
    Result,
};
use ink_metadata::{
    ConstructorSpec,
    InkProject,
    MessageSpec,
};
use scale::{
    Compact,
    Decode,
    Input,
};
use scale_info::{
    form::{
        Form,
        PortableForm,
    },
    Field,
};
use std::{
    fmt::Debug,
    path::Path,
};

/// Encode strings to SCALE encoded smart contract calls.
/// Decode SCALE encoded smart contract events and return values into `Value` objects.
pub struct ContractMessageTranscoder {
    metadata: InkProject,
    transcoder: Transcoder,
}

impl ContractMessageTranscoder {
    pub fn new(metadata: InkProject) -> Self {
        let transcoder = TranscoderBuilder::new(metadata.registry())
            .register_custom_type_transcoder::<<ink_env::DefaultEnvironment as ink_env::Environment>::AccountId, _>(env_types::AccountId)
            .register_custom_type_decoder::<<ink_env::DefaultEnvironment as ink_env::Environment>::Hash, _>(env_types::Hash)
            .done();
        Self {
            metadata,
            transcoder,
        }
    }

    /// Attempt to create a [`ContractMessageTranscoder`] from the metadata file at the given path.
    pub fn load<P>(metadata_path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let path = metadata_path.as_ref();
        let metadata: contract_metadata::ContractMetadata =
            contract_metadata::ContractMetadata::load(&metadata_path)?;
        let ink_metadata = serde_json::from_value(serde_json::Value::Object(
            metadata.abi,
        ))
        .context(format!(
            "Failed to deserialize ink project metadata from file {}",
            path.display()
        ))?;

        Ok(Self::new(ink_metadata))
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
            self.transcoder.encode(
                self.metadata.registry(),
                spec.ty().ty().id(),
                &value,
                &mut encoded,
            )?;
        }
        Ok(encoded)
    }

    pub fn decode(&self, type_id: u32, input: &mut &[u8]) -> Result<Value> {
        self.transcoder
            .decode(self.metadata.registry(), type_id, input)
    }

    fn constructors(&self) -> impl Iterator<Item = &ConstructorSpec<PortableForm>> {
        self.metadata.spec().constructors().iter()
    }

    fn messages(&self) -> impl Iterator<Item = &MessageSpec<PortableForm>> {
        self.metadata.spec().messages().iter()
    }

    fn find_message_spec(&self, name: &str) -> Option<&MessageSpec<PortableForm>> {
        self.messages().find(|msg| msg.label() == &name.to_string())
    }

    fn find_constructor_spec(
        &self,
        name: &str,
    ) -> Option<&ConstructorSpec<PortableForm>> {
        self.constructors()
            .find(|msg| msg.label() == &name.to_string())
    }

    pub fn decode_contract_event(&self, data: &mut &[u8]) -> Result<Value> {
        // data is an encoded `Vec<u8>` so is prepended with its length `Compact<u32>`, which we
        // ignore because the structure of the event data is known for decoding.
        let _len = <Compact<u32>>::decode(data)?;
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
        tracing::debug!("Decoding contract event '{}'", event_spec.label());

        let mut args = Vec::new();
        for arg in event_spec.args() {
            let name = arg.label().to_string();
            let value = self.decode(arg.ty().ty().id(), data)?;
            args.push((Value::String(name), value));
        }

        let name = event_spec.label().to_string();
        let map = Map::new(Some(&name), args.into_iter().collect());

        Ok(Value::Map(map))
    }

    pub fn decode_contract_message(&self, data: &mut &[u8]) -> Result<Value> {
        let mut msg_selector = [0u8; 4];
        data.read(&mut msg_selector)?;
        let msg_spec = self
            .messages()
            .find(|x| msg_selector == x.selector().to_bytes())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Message with selector {} not found in contract metadata",
                    hex::encode(msg_selector)
                )
            })?;
        tracing::debug!("Decoding contract message '{}'", msg_spec.label());

        let mut args = Vec::new();
        for arg in msg_spec.args() {
            let name = arg.label().to_string();
            let value = self.decode(arg.ty().ty().id(), data)?;
            args.push((Value::String(name), value));
        }

        let name = msg_spec.label().to_string();
        let map = Map::new(Some(&name), args.into_iter().collect());

        Ok(Value::Map(map))
    }

    pub fn decode_contract_constructor(&self, data: &mut &[u8]) -> Result<Value> {
        let mut msg_selector = [0u8; 4];
        data.read(&mut msg_selector)?;
        let msg_spec = self
            .constructors()
            .find(|x| msg_selector == x.selector().to_bytes())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Constructor with selector {} not found in contract metadata",
                    hex::encode(msg_selector)
                )
            })?;
        tracing::debug!("Decoding contract constructor '{}'", msg_spec.label());

        let mut args = Vec::new();
        for arg in msg_spec.args() {
            let name = arg.label().to_string();
            let value = self.decode(arg.ty().ty().id(), data)?;
            args.push((Value::String(name), value));
        }

        let name = msg_spec.label().to_string();
        let map = Map::new(Some(&name), args.into_iter().collect());

        Ok(Value::Map(map))
    }

    pub fn decode_return(&self, name: &str, data: &mut &[u8]) -> Result<Value> {
        let msg_spec = self.find_message_spec(name).ok_or_else(|| {
            anyhow::anyhow!("Failed to find message spec with name '{}'", name)
        })?;
        if let Some(return_ty) = msg_spec.return_type().opt_type() {
            self.decode(return_ty.ty().id(), data)
        } else {
            Ok(Value::Unit)
        }
    }
}

impl TryFrom<contract_metadata::ContractMetadata> for ContractMessageTranscoder {
    type Error = anyhow::Error;

    fn try_from(
        metadata: contract_metadata::ContractMetadata,
    ) -> Result<Self, Self::Error> {
        Ok(Self::new(serde_json::from_value(
            serde_json::Value::Object(metadata.abi),
        )?))
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
                .map(|field| {
                    CompositeTypeNamedField {
                        name: field
                            .name()
                            .expect("All fields have a name; qed")
                            .to_owned(),
                        field: field.clone(),
                    }
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

    use crate::scon::Hex;

    #[allow(clippy::extra_unused_lifetimes)]
    #[ink::contract]
    pub mod transcode {
        #[ink(storage)]
        pub struct Transcode {
            value: bool,
        }

        #[ink(event)]
        pub struct Event1 {
            #[ink(topic)]
            name: Hash,
            #[ink(topic)]
            from: AccountId,
        }

        impl Transcode {
            #[ink(constructor)]
            pub fn new(init_value: bool) -> Self {
                Self { value: init_value }
            }

            #[ink(constructor)]
            pub fn default() -> Self {
                Self::new(Default::default())
            }

            #[ink(message)]
            pub fn flip(&mut self) {
                self.value = !self.value;
            }

            #[ink(message)]
            pub fn get(&self) -> bool {
                self.value
            }

            #[ink(message)]
            pub fn set_account_id(&self, account_id: AccountId) {
                let _ = account_id;
            }

            #[ink(message)]
            pub fn set_account_ids_vec(&self, account_ids: Vec<AccountId>) {
                let _ = account_ids;
            }

            #[ink(message)]
            pub fn primitive_vec_args(&self, args: Vec<u32>) {
                let _ = args;
            }

            #[ink(message)]
            pub fn uint_args(
                &self,
                _u8: u8,
                _u16: u16,
                _u32: u32,
                _u64: u64,
                _u128: u128,
            ) {
            }

            #[ink(message)]
            pub fn uint_array_args(&self, arr: [u8; 4]) {
                let _ = arr;
            }
        }
    }

    fn generate_metadata() -> InkProject {
        extern "Rust" {
            fn __ink_generate_metadata() -> InkProject;
        }

        unsafe { __ink_generate_metadata() }
    }

    #[test]
    fn encode_single_primitive_arg() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(metadata);

        let encoded = transcoder.encode("new", ["true"])?;
        // encoded args follow the 4 byte selector
        let encoded_args = &encoded[4..];

        assert_eq!(true.encode(), encoded_args);
        Ok(())
    }

    #[test]
    fn encode_account_id_custom_ss58_encoding() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(metadata);

        let encoded = transcoder.encode(
            "set_account_id",
            ["5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"],
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
    fn encode_account_ids_vec_args() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(metadata);

        let encoded = transcoder.encode(
            "set_account_ids_vec",
            ["[5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY, 5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty]"],
        )?;

        // encoded args follow the 4 byte selector
        let encoded_args = &encoded[4..];

        let expected = vec![
            sp_core::crypto::AccountId32::from_str(
                "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
            )
            .unwrap(),
            sp_core::crypto::AccountId32::from_str(
                "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty",
            )
            .unwrap(),
        ];
        assert_eq!(expected.encode(), encoded_args);
        Ok(())
    }

    #[test]
    fn encode_primitive_vec_args() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(metadata);

        let encoded = transcoder.encode("primitive_vec_args", ["[1, 2]"])?;

        // encoded args follow the 4 byte selector
        let encoded_args = &encoded[4..];

        let expected = vec![1, 2];
        assert_eq!(expected.encode(), encoded_args);
        Ok(())
    }

    #[test]
    fn encode_uint_hex_literals() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(metadata);

        let encoded = transcoder.encode(
            "uint_args",
            [
                "0x00",
                "0xDEAD",
                "0xDEADBEEF",
                "0xDEADBEEF12345678",
                "0xDEADBEEF0123456789ABCDEF01234567",
            ],
        )?;

        // encoded args follow the 4 byte selector
        let encoded_args = &encoded[4..];

        let expected = (
            0x00u8,
            0xDEADu16,
            0xDEADBEEFu32,
            0xDEADBEEF12345678u64,
            0xDEADBEEF0123456789ABCDEF01234567u128,
        );
        assert_eq!(expected.encode(), encoded_args);
        Ok(())
    }

    #[test]
    fn encode_uint_arr_hex_literals() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(metadata);

        let encoded =
            transcoder.encode("uint_array_args", ["[0xDE, 0xAD, 0xBE, 0xEF]"])?;

        // encoded args follow the 4 byte selector
        let encoded_args = &encoded[4..];

        let expected: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];
        assert_eq!(expected.encode(), encoded_args);
        Ok(())
    }

    #[test]
    fn decode_primitive_return() {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(metadata);

        let encoded = Result::<bool, ink::primitives::LangError>::Ok(true).encode();
        let decoded = transcoder
            .decode_return("get", &mut &encoded[..])
            .unwrap_or_else(|e| panic!("Error decoding return value {}", e));

        let expected = Value::Tuple(Tuple::new(
            "Ok".into(),
            [Value::Bool(true)].into_iter().collect(),
        ));
        assert_eq!(expected, decoded);
    }

    #[test]
    fn decode_lang_error() {
        use ink::primitives::LangError;

        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(metadata);

        let encoded =
            Result::<bool, LangError>::Err(LangError::CouldNotReadInput).encode();
        let decoded = transcoder
            .decode_return("get", &mut &encoded[..])
            .unwrap_or_else(|e| panic!("Error decoding return value {}", e));

        let expected = Value::Tuple(Tuple::new(
            "Err".into(),
            [Value::Tuple(Tuple::new(
                Some("CouldNotReadInput"),
                Vec::new(),
            ))]
            .to_vec(),
        ));
        assert_eq!(expected, decoded);
    }

    #[test]
    fn decode_contract_event() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(metadata);

        // raw encoded event with event index prefix
        let encoded = (0u8, [0u32; 32], [1u32; 32]).encode();
        // encode again as a Vec<u8> which has a len prefix.
        let encoded_bytes = encoded.encode();
        let _ = transcoder.decode_contract_event(&mut &encoded_bytes[..])?;

        Ok(())
    }

    #[test]
    fn decode_hash_as_hex_encoded_string() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(metadata);

        let hash = [
            52u8, 40, 235, 225, 70, 245, 184, 36, 21, 218, 130, 114, 75, 207, 117, 240,
            83, 118, 135, 56, 220, 172, 95, 131, 171, 125, 130, 167, 10, 15, 242, 222,
        ];
        // raw encoded event with event index prefix
        let encoded = (0u8, hash, [0u32; 32]).encode();
        // encode again as a Vec<u8> which has a len prefix.
        let encoded_bytes = encoded.encode();
        let decoded = transcoder.decode_contract_event(&mut &encoded_bytes[..])?;

        if let Value::Map(ref map) = decoded {
            let name_field = &map[&Value::String("name".into())];
            if let Value::Hex(hex) = name_field {
                assert_eq!(&Hex::from_str("0x3428ebe146f5b82415da82724bcf75f053768738dcac5f83ab7d82a70a0ff2de")?, hex);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Expected a name field hash encoded as Hex value, was {:?}",
                    name_field
                ))
            }
        } else {
            Err(anyhow::anyhow!(
                "Expected a Value::Map for the decoded event"
            ))
        }
    }

    #[test]
    fn decode_contract_message() -> Result<()> {
        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(metadata);

        let encoded_bytes = hex::decode("633aa551").unwrap();
        let _ = transcoder.decode_contract_message(&mut &encoded_bytes[..])?;

        Ok(())
    }
}

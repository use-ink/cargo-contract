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

use super::{
    BalanceVariant,
    TokenMetadata,
};
use crate::DEFAULT_KEY_COL_WIDTH;
use colored::Colorize as _;
use contract_build::Verbosity;
use contract_transcode::{
    ContractMessageTranscoder,
    Hex,
    TranscoderBuilder,
    Value,
};

use anyhow::Result;
use ink_env::Environment;
use scale::Encode;
use scale_info::form::PortableForm;
use std::{
    fmt::{
        Display,
        Write,
    },
    str::FromStr,
};
use subxt::{
    self,
    Config,
    blocks::ExtrinsicEvents,
    events::StaticEvent,
    ext::{
        scale_decode::{
            self,
            IntoVisitor,
        },
        scale_encode,
    },
    utils::{
        H160,
        H256,
    },
};

/// A custom event emitted by the contract.
#[derive(
    scale::Decode,
    scale::Encode,
    scale_decode::DecodeAsType,
    scale_encode::EncodeAsType,
    Debug,
)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct ContractEmitted {
    /// The contract that emitted the event.
    contract: H160,
    /// Data supplied by the contract. Metadata generated during contract compilation
    /// is needed to decode it.
    data: Vec<u8>,
    // A list of topics used to index the event.
    // Number of topics is capped by [`limits::NUM_EVENT_TOPICS`].
    topics: Vec<H256>,
}

impl StaticEvent for ContractEmitted {
    const PALLET: &'static str = "Revive";
    const EVENT: &'static str = "ContractEmitted";
}

/// Contract deployed by deployer at the specified address.
#[derive(
    scale::Decode,
    scale::Encode,
    scale_decode::DecodeAsType,
    scale_encode::EncodeAsType,
    Debug,
)]
#[decode_as_type(crate_path = "subxt::ext::scale_decode")]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct ContractInstantiated {
    /// Address of the deployer.
    pub deployer: H160,
    /// Address where the contract was instantiated to.
    pub contract: H160,
}

impl StaticEvent for ContractInstantiated {
    const PALLET: &'static str = "Revive";
    const EVENT: &'static str = "Instantiated";
}

/// Field that represent data of an event from invoking a contract extrinsic.
#[derive(serde::Serialize)]
pub struct Field {
    /// name of a field
    pub name: String,
    /// value of a field
    pub value: Value,
    /// The name of a type as defined in the pallet Source Code
    #[serde(skip_serializing)]
    pub type_name: Option<String>,
}

impl Field {
    pub fn new(name: String, value: Value, type_name: Option<String>) -> Self {
        Field {
            name,
            value,
            type_name,
        }
    }
}

/// An event produced from invoking a contract extrinsic.
#[derive(serde::Serialize)]
pub struct Event {
    /// name of a pallet
    pub pallet: String,
    /// name of the event
    pub name: String,
    /// data associated with the event
    pub fields: Vec<Field>,
}

/// Events produced from invoking a contract extrinsic.
#[derive(serde::Serialize)]
#[allow(dead_code)]
pub struct Events(Vec<Event>);

/// Displays events produced from invoking a contract extrinsic.
#[derive(serde::Serialize)]
pub struct DisplayEvents(Vec<Event>);

#[allow(clippy::needless_borrows_for_generic_args)]
impl DisplayEvents {
    /// Parses events and returns an object which can be serialised
    pub fn from_events<C: Config, E: Environment>(
        result: &ExtrinsicEvents<C>,
        transcoder: Option<&ContractMessageTranscoder>,
        subxt_metadata: &subxt::Metadata,
    ) -> Result<DisplayEvents>
    where
        C::AccountId: IntoVisitor,
    {
        let mut events: Vec<Event> = vec![];

        let events_transcoder = TranscoderBuilder::new(subxt_metadata.types())
            .with_default_custom_type_transcoders()
            .done();

        for event in result.iter() {
            let event = event?;
            tracing::debug!(
                "displaying event {}:{}",
                event.pallet_name(),
                event.variant_name()
            );

            let event_metadata = event.event_metadata();
            let event_fields = &event_metadata.variant.fields;

            let mut event_entry = Event {
                pallet: event.pallet_name().to_string(),
                name: event.variant_name().to_string(),
                fields: vec![],
            };

            // For ContractEmitted events, decode to get the event signature topic and
            // data
            let contract_emitted = if <ContractEmitted as StaticEvent>::is_event(
                event.pallet_name(),
                event.variant_name(),
            ) {
                event.as_event::<ContractEmitted>().ok().flatten()
            } else {
                None
            };

            let event_data = &mut event.field_bytes();
            tracing::debug!("event data: {:?}", hex::encode(&event_data));
            let mut unnamed_field_name = 0;
            for field_metadata in event_fields {
                if let Some(ref ce) = contract_emitted {
                    if field_metadata.name == Some("data".to_string()) {
                        // Decode the contract event data using the transcoder.
                        // The transcoder expects the data to be prefixed with its length
                        // as Compact<u32>.
                        let mut encoded_data =
                            scale::Compact(ce.data.len() as u32).encode();
                        encoded_data.extend_from_slice(&ce.data);
                        let mut data_slice = encoded_data.as_slice();
                        let field = contract_event_vec_field(
                            transcoder,
                            field_metadata,
                            ce.topics.first(),
                            &mut data_slice,
                            field_metadata.name.as_ref().expect("must exist"),
                        )?;
                        event_entry.fields.push(field);
                    } else if field_metadata.name == Some("topics".to_string()) {
                        // Skip the topics field or display it as hex
                        // Topics are already used for event signature matching
                        continue;
                    } else {
                        // Non-data/topics fields in ContractEmitted (e.g., contract
                        // address)
                        let field_name = field_metadata
                            .name
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| {
                                let name = unnamed_field_name.to_string();
                                unnamed_field_name += 1;
                                name
                            });

                        let decoded_field = events_transcoder.decode(
                            subxt_metadata.types(),
                            field_metadata.ty.id,
                            event_data,
                        )?;
                        let field = Field::new(
                            field_name,
                            decoded_field,
                            field_metadata.type_name.as_ref().map(|s| s.to_string()),
                        );
                        event_entry.fields.push(field);
                    }
                } else {
                    let field_name = field_metadata
                        .name
                        .as_ref()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            let name = unnamed_field_name.to_string();
                            unnamed_field_name += 1;
                            name
                        });

                    let decoded_field = events_transcoder.decode(
                        subxt_metadata.types(),
                        field_metadata.ty.id,
                        event_data,
                    )?;
                    let field = Field::new(
                        field_name,
                        decoded_field,
                        field_metadata.type_name.as_ref().map(|s| s.to_string()),
                    );
                    event_entry.fields.push(field);
                }
            }
            events.push(event_entry);
        }

        Ok(DisplayEvents(events))
    }

    /// Displays events in a human readable format
    pub fn display_events<E: Environment>(
        &self,
        verbosity: Verbosity,
        token_metadata: &TokenMetadata,
    ) -> Result<String>
    where
        E::Balance: Display + From<u128>,
    {
        let event_field_indent: usize = DEFAULT_KEY_COL_WIDTH - 3;
        let mut out = format!(
            "{:>width$}\n",
            "Events".bright_purple().bold(),
            width = DEFAULT_KEY_COL_WIDTH
        );
        for event in &self.0 {
            let _ = writeln!(
                out,
                "{:>width$} {} ➜ {}",
                "Event".bright_green().bold(),
                event.pallet.bright_white(),
                event.name.bright_white().bold(),
                width = DEFAULT_KEY_COL_WIDTH
            );

            for field in &event.fields {
                if verbosity.is_verbose() {
                    let mut value: String = field.value.to_string();
                    if (field.type_name == Some("T::Balance".to_string())
                        || field.type_name == Some("BalanceOf<T>".to_string()))
                        && let Value::UInt(balance) = field.value
                    {
                        value = BalanceVariant::<E::Balance>::from(
                            balance,
                            Some(token_metadata),
                        )?
                        .to_string();
                    }
                    if field.type_name == Some("H160".to_string()) {
                        // Value is in the format `H160([bytes])`.
                        // Extract the byte array between the brackets and convert it to a
                        // hexadecimal string.
                        if let (Some(start), Some(end)) =
                            (value.find('['), value.find(']'))
                        {
                            let byte_str = &value[start + 1..end];
                            let bytes: Vec<u8> = byte_str
                                .split(", ")
                                .filter_map(|s| s.parse::<u8>().ok())
                                .collect();
                            let h160_value = H160::from_slice(&bytes);
                            value = format!("0x{}", hex::encode(h160_value.as_bytes()));
                        }
                    }
                    let _ = writeln!(
                        out,
                        "{:width$}{}: {}",
                        "",
                        field.name.bright_white(),
                        value,
                        width = event_field_indent,
                    );
                }
            }
        }
        Ok(out)
    }

    /// Returns an event result in json format
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

/// Construct the contract event data field, attempting to decode the event using the
/// [`ContractMessageTranscoder`] if available.
#[allow(clippy::needless_borrows_for_generic_args)]
fn contract_event_vec_field(
    transcoder: Option<&ContractMessageTranscoder>,
    field_metadata: &scale_info::Field<PortableForm>,
    event_sig_topic: Option<&H256>,
    event_data: &mut &[u8],
    field_name: &String,
) -> Result<Field> {
    let event_value = if let Some(transcoder) = transcoder {
        if let Some(event_sig_topic) = event_sig_topic {
            match transcoder.decode_contract_event(event_sig_topic, event_data) {
                Ok(contract_event) => contract_event,
                Err(err) => {
                    tracing::warn!(
                        "Decoding contract event failed: {:?}. It might have come from another contract.",
                        err
                    );
                    Value::Hex(Hex::from_str(&hex::encode(&event_data))?)
                }
            }
        } else {
            tracing::info!("Anonymous event not decoded. Data displayed as raw hex.");
            Value::Hex(Hex::from_str(&hex::encode(event_data))?)
        }
    } else {
        Value::Hex(Hex::from_str(&hex::encode(event_data))?)
    };
    Ok(Field::new(
        field_name.to_string(),
        event_value,
        field_metadata.type_name.as_ref().map(|s| s.to_string()),
    ))
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use anyhow::Result;
    use ink::{
        metadata::InkProject,
        prelude::vec::Vec,
    };
    use ink_env::Event as _;
    use scale::Encode;
    use scale_info::{
        Field as ScaleField,
        IntoPortable as _,
    };
    use subxt::utils::H256;

    #[allow(clippy::extra_unused_lifetimes, unexpected_cfgs, non_local_definitions)]
    #[ink::contract]
    pub mod event_contract {
        #[ink(storage)]
        pub struct EventHarness {}

        #[ink(event)]
        pub struct BalanceChanged {
            pub value: bool,
            pub amount: u32,
        }

        impl Default for EventHarness {
            fn default() -> Self {
                Self::new()
            }
        }

        impl EventHarness {
            #[ink(constructor)]
            pub fn new() -> Self {
                Self {}
            }

            #[ink(message)]
            pub fn touch(&self) {}
        }
    }

    fn contract_data_field_metadata() -> scale_info::Field<PortableForm> {
        let meta_field = ScaleField::new(
            Some("data"),
            scale_info::MetaType::new::<Vec<u8>>(),
            Some("Vec<u8>"),
            vec![],
        );
        let mut registry = scale_info::Registry::new();
        meta_field.into_portable(&mut registry)
    }

    fn generate_metadata() -> InkProject {
        unsafe extern "Rust" {
            fn __ink_generate_metadata() -> InkProject;
        }
        unsafe { __ink_generate_metadata() }
    }

    /// Without a transcoder we fall back to raw hex representation.
    #[test]
    fn contract_event_without_transcoder_returns_hex() {
        let field_meta = contract_data_field_metadata();

        // Sample event data (would normally be SCALE-encoded event fields)
        let event_data_bytes = vec![0x04, 0x00, 0x01, 0x02, 0x03];
        let mut event_data = event_data_bytes.as_slice();

        let result = contract_event_vec_field(
            None,
            &field_meta,
            None,
            &mut event_data,
            &"data".to_string(),
        );

        assert!(result.is_ok(), "decoding without transcoder should succeed");
        let field = result.unwrap();

        match field.value {
            Value::Hex(_) => {}
            other => panic!("expected raw hex fallback, got {other:?}"),
        }
    }

    /// With a transcoder the contract event data is decoded into its fields.
    #[test]
    fn contract_event_with_transcoder_decodes_payload() -> Result<()> {
        let field_meta = contract_data_field_metadata();

        let metadata = generate_metadata();
        let transcoder = ContractMessageTranscoder::new(metadata);

        let payload = event_contract::BalanceChanged {
            value: true,
            amount: 7u32,
        };
        let payload_bytes = payload.encode();
        let mut encoded_data = scale::Compact(payload_bytes.len() as u32).encode();
        encoded_data.extend_from_slice(&payload_bytes);
        let mut data_slice = encoded_data.as_slice();

        let signature_topic_bytes = event_contract::BalanceChanged::SIGNATURE_TOPIC
            .expect("event has a signature topic");
        let signature_topic = H256::from(signature_topic_bytes);

        let field = contract_event_vec_field(
            Some(&transcoder),
            &field_meta,
            Some(&signature_topic),
            &mut data_slice,
            &"data".to_string(),
        )?;

        let Value::Map(map) = field.value else {
            panic!("expected decoded event to be a map");
        };
        let decoded_value = map
            .get_by_str("value")
            .expect("decoded event contains `value` field");
        assert_eq!(decoded_value, &Value::Bool(true));
        let decoded_amount = map
            .get_by_str("amount")
            .expect("decoded event contains `amount` field");
        assert_eq!(decoded_amount, &Value::UInt(7u128));

        Ok(())
    }
}

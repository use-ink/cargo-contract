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
    blocks::ExtrinsicEvents,
    events::StaticEvent,
    ext::{
        scale_decode::{
            self,
            IntoVisitor,
        },
        scale_encode,
    },
    utils::H160,
    Config,
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
pub struct ContractEmitted<AccountId> {
    pub contract: AccountId,
    pub data: Vec<u8>,
}

impl<AccountId> StaticEvent for ContractEmitted<AccountId>
where
    AccountId: IntoVisitor,
{
    const PALLET: &'static str = "Revive";
    const EVENT: &'static str = "ContractEmitted";
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

            let event_data = &mut event.field_bytes();
            let event_sig_topic = event.topics().iter().next();
            let mut unnamed_field_name = 0;
            for field_metadata in event_fields {
                if <ContractEmitted<C::AccountId> as StaticEvent>::is_event(
                    event.pallet_name(),
                    event.variant_name(),
                ) && field_metadata.name == Some("data".to_string())
                {
                    tracing::debug!("event data: {:?}", hex::encode(&event_data));
                    let field = contract_event_data_field::<C>(
                        transcoder,
                        field_metadata,
                        event_sig_topic,
                        event_data,
                    )?;
                    event_entry.fields.push(field);
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
                    if field.type_name == Some("T::Balance".to_string())
                        || field.type_name == Some("BalanceOf<T>".to_string())
                    {
                        if let Value::UInt(balance) = field.value {
                            value = BalanceVariant::<E::Balance>::from(
                                balance,
                                Some(token_metadata),
                            )?
                            .to_string();
                        }
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
fn contract_event_data_field<C: Config>(
    transcoder: Option<&ContractMessageTranscoder>,
    field_metadata: &scale_info::Field<PortableForm>,
    event_sig_topic: Option<&C::Hash>,
    event_data: &mut &[u8],
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
        String::from("data"),
        event_value,
        field_metadata.type_name.as_ref().map(|s| s.to_string()),
    ))
}

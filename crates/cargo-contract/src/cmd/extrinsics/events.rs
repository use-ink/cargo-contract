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
    runtime_api::api::contracts::events::ContractEmitted,
    BalanceVariant,
    DefaultConfig,
    TokenMetadata,
};
use crate::{
    Verbosity,
    DEFAULT_KEY_COL_WIDTH,
};
use colored::Colorize as _;
use transcode::{
    ContractMessageTranscoder,
    TranscoderBuilder,
    Value,
};

use anyhow::Result;
use std::fmt::Write;
use subxt::{
    self,
    blocks::ExtrinsicEvents,
    events::StaticEvent,
};

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

/// An event produced from from invoking a contract extrinsic.
#[derive(serde::Serialize)]
pub struct Event {
    /// name of a pallet
    pub pallet: String,
    /// name of the event
    pub name: String,
    /// data associated with the event
    pub fields: Vec<Field>,
}

/// Displays events produced from invoking a contract extrinsic.
#[derive(serde::Serialize)]
pub struct DisplayEvents(Vec<Event>);

impl DisplayEvents {
    /// Parses events and returns an object which can be serialised
    pub fn from_events(
        result: &ExtrinsicEvents<DefaultConfig>,
        transcoder: &ContractMessageTranscoder,
        subxt_metadata: &subxt::Metadata,
    ) -> Result<DisplayEvents> {
        let mut events: Vec<Event> = vec![];

        let runtime_metadata = subxt_metadata.runtime_metadata();
        let events_transcoder = TranscoderBuilder::new(&runtime_metadata.types)
            .with_default_custom_type_transcoders()
            .done();

        for event in result.iter() {
            let event = event?;
            tracing::debug!("displaying event {:?}", event);

            let event_metadata =
                subxt_metadata.event(event.pallet_index(), event.variant_index())?;
            let event_fields = event_metadata.fields();

            let mut event_entry = Event {
                pallet: event.pallet_name().to_string(),
                name: event.variant_name().to_string(),
                fields: vec![],
            };

            let event_data = &mut event.field_bytes();
            let mut unnamed_field_name = 0;
            for field_metadata in event_fields {
                if <ContractEmitted as StaticEvent>::is_event(
                    event.pallet_name(),
                    event.variant_name(),
                ) && field_metadata.name() == Some("data")
                {
                    tracing::debug!("event data: {:?}", hex::encode(&event_data));
                    match transcoder.decode_contract_event(event_data) {
                        Ok(contract_event) => {
                            let field = Field::new(
                                String::from("data"),
                                contract_event,
                                field_metadata.type_name().map(|s| s.to_string()),
                            );
                            event_entry.fields.push(field);
                        }
                        Err(err) => {
                            tracing::warn!(
                                "Decoding contract event failed: {:?}. It might have come from another contract.",
                                err
                            )
                        }
                    }
                } else {
                    let field_name = field_metadata
                        .name()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            let name = unnamed_field_name.to_string();
                            unnamed_field_name += 1;
                            name
                        });

                    let decoded_field = events_transcoder.decode(
                        &runtime_metadata.types,
                        field_metadata.type_id(),
                        event_data,
                    )?;
                    let field = Field::new(
                        field_name,
                        decoded_field,
                        field_metadata.type_name().map(|s| s.to_string()),
                    );
                    event_entry.fields.push(field);
                }
            }
            events.push(event_entry);
        }

        Ok(DisplayEvents(events))
    }

    /// Displays events in a human readable format
    pub fn display_events(
        &self,
        verbosity: Verbosity,
        token_metadata: &TokenMetadata,
    ) -> Result<String> {
        let event_field_indent: usize = DEFAULT_KEY_COL_WIDTH - 3;
        let mut out = format!(
            "{:>width$}\n",
            "Events".bold(),
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
                            value = BalanceVariant::from(balance, Some(token_metadata))?
                                .to_string();
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

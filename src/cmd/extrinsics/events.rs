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
    DefaultConfig,
};
use crate::{
    OutputType,
    Verbosity,
    DEFAULT_KEY_COL_WIDTH,
};
use colored::Colorize as _;
use transcode::{
    ContractMessageTranscoder,
    TranscoderBuilder,
    Value,
};

use anyhow::{
    Ok,
    Result,
};
use std::fmt::Write;
use subxt::{
    self,
    events::StaticEvent,
    tx::TxEvents,
};

/// Field that represent data of the event from contract call
#[derive(serde::Serialize)]
pub struct Field {
    /// name of a field
    pub name: String,
    /// value of a field
    pub value: Value,
}

impl Field {
    pub fn new(name: String, value: Value) -> Self {
        Field { name, value }
    }
}

/// Events produced from calling a contract
#[derive(serde::Serialize)]
pub struct Event {
    /// name of a pallet
    pub pallet: String,
    /// name of the event
    pub name: String,
    /// data associated with the event
    pub fields: Vec<Field>,
}

/// Result of the contract call
#[derive(serde::Serialize)]
pub struct CallResult {
    /// Events that were produced from calling a contract
    pub events: Vec<Event>,
    /// The type of formatting to use for the build output.
    #[serde(skip_serializing)]
    pub output_type: OutputType,
}

impl CallResult {
    pub fn display(&self, verbosity: &Verbosity) -> String {
        let event_field_indent: usize = DEFAULT_KEY_COL_WIDTH - 3;
        let mut out = format!("{}\n", "Events".bold());
        for event in &self.events {
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
                    let _ = writeln!(
                        out,
                        "{:width$}{}: {}",
                        "",
                        field.name.bright_white(),
                        field.value,
                        width = event_field_indent,
                    );
                }
            }
        }
        out
    }
}

/// Parses events and returns an object which can be serialised
pub fn parse_events(
    result: &TxEvents<DefaultConfig>,
    transcoder: &ContractMessageTranscoder,
    subxt_metadata: &subxt::Metadata,
    output_type: OutputType,
) -> Result<CallResult> {
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
        for (field, field_ty) in event_fields {
            if <ContractEmitted as StaticEvent>::is_event(
                event.pallet_name(),
                event.variant_name(),
            ) && field.as_ref() == Some(&"data".to_string())
            {
                tracing::debug!("event data: {:?}", hex::encode(&event_data));
                let contract_event = transcoder.decode_contract_event(event_data)?;
                let field = Field::new(String::from("data"), contract_event);
                event_entry.fields.push(field);
            } else {
                let field_name = field.clone().unwrap_or_else(|| {
                    let name = unnamed_field_name.to_string();
                    unnamed_field_name += 1;
                    name
                });

                let decoded_field = events_transcoder.decode(
                    &runtime_metadata.types,
                    *field_ty,
                    event_data,
                )?;
                let field = Field::new(field_name, decoded_field);
                event_entry.fields.push(field);
            }
        }
        events.push(event_entry);
    }

    Ok(CallResult {
        events,
        output_type,
    })
}

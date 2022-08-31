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
    maybe_println,
    Verbosity,
    DEFAULT_KEY_COL_WIDTH,
};
use colored::Colorize as _;
use transcode::{
    ContractMessageTranscoder,
    TranscoderBuilder, Value,
};

use anyhow::Result;
use subxt::{
    self,
    events::StaticEvent,
    tx::TxEvents,
};

/// Field that represent data of the event from contract call
#[derive(Debug, Clone, serde::Serialize)]
pub struct Field {
    pub name: String,
    pub value: Value
}

/// Events produced from calling a contract
#[derive(Debug, Clone, serde::Serialize)]
pub struct Event {
    pub name: String,
    pub fields: Vec<Field>
}

/// Result of the contract call
#[derive(Debug, Clone, serde::Serialize)]
pub struct CallResult {
    pub events: Vec<Event>
}

pub fn display_events(
    result: &TxEvents<DefaultConfig>,
    transcoder: &ContractMessageTranscoder,
    subxt_metadata: &subxt::Metadata,
    verbosity: &Verbosity,
) -> Result<()> {
    if matches!(verbosity, Verbosity::Quiet) {
        return Ok(())
    }

    println!();

    if matches!(verbosity, Verbosity::Verbose) {
        println!("VERBOSE")
    }

    let runtime_metadata = subxt_metadata.runtime_metadata();
    let events_transcoder = TranscoderBuilder::new(&runtime_metadata.types)
        .with_default_custom_type_transcoders()
        .done();

    const EVENT_FIELD_INDENT: usize = DEFAULT_KEY_COL_WIDTH - 3;

    for event in result.iter() {
        let event = event?;
        tracing::debug!("displaying event {:?}", event);

        let event_metadata =
            subxt_metadata.event(event.pallet_index(), event.variant_index())?;
        let event_fields = event_metadata.fields();

        println!(
            "{:>width$} {} ➜ {}",
            "Event".bright_green().bold(),
            event.pallet_name().bright_white(),
            event.variant_name().bright_white().bold(),
            width = DEFAULT_KEY_COL_WIDTH
        );
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
                maybe_println!(
                    verbosity,
                    "{:width$}{}",
                    "",
                    format!("{}: {}", "data".bright_white(), contract_event),
                    width = EVENT_FIELD_INDENT
                );
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
                maybe_println!(
                    verbosity,
                    "{:width$}{}",
                    "",
                    format!("{}: {}", field_name.bright_white(), decoded_field),
                    width = EVENT_FIELD_INDENT
                );
            }
        }
    }
    println!();
    Ok(())
}

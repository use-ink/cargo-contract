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
    pretty_print,
    runtime_api::{api, ContractsRuntime},
    transcode::{env_types, ContractMessageTranscoder, TranscoderBuilder},
};
use crate::Verbosity;

use anyhow::Result;
use colored::Colorize;
use std::fmt::{Display, Formatter, Result as FmtResult};
use subxt::{self, Event, ExtrinsicSuccess, RawEvent};

pub fn display_events(
    result: &ExtrinsicSuccess<ContractsRuntime>,
    transcoder: &ContractMessageTranscoder,
    subxt_metadata: &subxt::Metadata,
    verbosity: Verbosity,
) -> Result<()> {
    if matches!(verbosity, Verbosity::Quiet) {
        return Ok(());
    }

    let runtime_metadata = subxt_metadata.runtime_metadata();
    let events_transcoder = TranscoderBuilder::new(&runtime_metadata.types)
        .register_custom_type::<sp_runtime::AccountId32, _>("AccountId", env_types::AccountId)
        .done();

    for event in &result.events {
        // todo display contract emitted events and special type formatting
        // print!(
        //     "{}::{} ",
        //     event.pallet.bold(),
        //     event.variant.bright_cyan().bold(),
        // );

        // if display_matching_event(
        //     event,
        //     |event| DisplayContractEmitted { transcoder, event },
        //     true,
        // ) {
        //     continue;
        // }

        let event_metadata = subxt_metadata.event(event.pallet_index, event.variant_index)?;
        let event_ident = format!("{}::{}", event.pallet, event.variant);
        let event_fields = event_metadata.variant().fields();
        let decoded_event = events_transcoder.decoder().decode_composite(
            Some(event_ident.as_str()),
            event_fields,
            &mut &event.data[..],
        )?;

        pretty_print(decoded_event, true);
        println!();
        log::info!(
            "{}::{} event has no matching custom display",
            event.pallet,
            event.variant
        );
    }
    println!();
    Ok(())
}

/// Prints the details for the given event if it matches.
///
/// Returns true iff the module and event name match.
fn display_matching_event<E, F, D>(raw_event: &RawEvent, new_display: F, indent: bool) -> bool
where
    E: Event,
    F: FnOnce(E) -> D,
    D: Display,
{
    if raw_event.pallet != E::PALLET || raw_event.variant != E::EVENT {
        return false;
    }

    match E::decode(&mut &raw_event.data[..]) {
        Ok(event) => {
            let display_event = new_display(event);
            let _ = pretty_print(display_event, indent);
        }
        Err(err) => {
            print!(
                "{} {}",
                "Error decoding event:".bright_red().bold(),
                format!("{}", err),
            );
        }
    }
    true
}

/// Wraps contracts::events::ContractEmitted for Display impl and decodes contract events.
struct DisplayContractEmitted<'a> {
    event: api::contracts::events::ContractEmitted,
    transcoder: &'a ContractMessageTranscoder<'a>,
}

impl<'a> Display for DisplayContractEmitted<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let mut builder = f.debug_struct("");
        builder.field("caller", &self.event.0);
        match self
            .transcoder
            .decode_contract_event(&mut &self.event.1[..])
        {
            Ok(contract_event) => {
                builder.field("event", &contract_event);
            }
            Err(err) => {
                log::error!("Error decoding contract event: {}", err);
                builder.field("event", &"Failed to decode contract event, see logs");
            }
        }
        builder.finish()
    }
}

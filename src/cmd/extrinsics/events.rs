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

use super::{pretty_print, Transcoder};
use crate::Verbosity;

use colored::Colorize;
use std::fmt::{Display, Formatter, Result};
use subxt::{
    balances, contracts, system, ContractsTemplateRuntime as Runtime, Event, ExtrinsicSuccess,
    RawEvent,
};

pub fn display_events(
    result: &ExtrinsicSuccess<Runtime>,
    transcoder: &Transcoder,
    verbosity: Verbosity,
) {
    if matches!(verbosity, Verbosity::Quiet) {
        return;
    }

    for event in &result.events {
        print!(
            "{}::{} ",
            event.module.bold(),
            event.variant.bright_cyan().bold(),
        );

        if display_matching_event(event, |e| DisplayExtrinsicSuccessEvent(e), false) {
            continue;
        }
        if display_matching_event(event, |e| DisplayExtrinsicFailedEvent(e), false) {
            continue;
        }
        if display_matching_event(event, |e| DisplayTransferEvent(e), true) {
            continue;
        }
        if display_matching_event(event, |e| DisplayNewAccountEvent(e), false) {
            continue;
        }
        if display_matching_event(event, |e| DisplayCodeStoredEvent(e), true) {
            continue;
        }
        if display_matching_event(event, |e| DisplayInstantiatedEvent(e), true) {
            continue;
        }
        if display_matching_event(
            event,
            |event| DisplayContractExecution { transcoder, event },
            true,
        ) {
            continue;
        }
        println!();
        log::info!(
            "{}::{} event has no matching custom display",
            event.module,
            event.variant
        );
    }
    println!();
}

/// Prints the details for the given event if it matches.
///
/// Returns true iff the module and event name match.
fn display_matching_event<E, F, D>(raw_event: &RawEvent, new_display: F, indent: bool) -> bool
where
    E: Event<Runtime>,
    F: FnOnce(E) -> D,
    D: Display,
{
    if raw_event.module != E::MODULE || raw_event.variant != E::EVENT {
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

/// Wraps ExtrinsicSuccessEvent for Display impl
struct DisplayExtrinsicSuccessEvent(system::ExtrinsicSuccessEvent<Runtime>);

impl Display for DisplayExtrinsicSuccessEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "")
    }
}

/// Wraps ExtrinsicFailedEvent for Display impl
struct DisplayExtrinsicFailedEvent(system::ExtrinsicFailedEvent<Runtime>);

impl Display for DisplayExtrinsicFailedEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut builder = f.debug_struct("");
        builder.field("error", &format!("{:?}", self.0.error));
        builder.finish()
    }
}

/// Wraps NewAccountEvent for Display impl
struct DisplayNewAccountEvent(system::NewAccountEvent<Runtime>);

impl Display for DisplayNewAccountEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut builder = f.debug_struct("");
        builder.field("account", &self.0.account);
        builder.finish()
    }
}

/// Wraps balances::TransferEvent for Display impl
struct DisplayTransferEvent(balances::TransferEvent<Runtime>);

impl Display for DisplayTransferEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut builder = f.debug_struct("");
        builder.field("from", &self.0.from);
        builder.field("to", &self.0.to);
        builder.field("amount", &self.0.amount);
        builder.finish()
    }
}

/// Wraps contracts::CodeStored for Display impl
struct DisplayCodeStoredEvent(contracts::CodeStoredEvent<Runtime>);

impl Display for DisplayCodeStoredEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut builder = f.debug_struct("");
        builder.field("code_hash", &self.0.code_hash);
        builder.finish()
    }
}

/// Wraps contracts::InstantiatedEvent for Display impl
struct DisplayInstantiatedEvent(contracts::InstantiatedEvent<Runtime>);

impl Display for DisplayInstantiatedEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut builder = f.debug_struct("");
        builder.field("caller", &self.0.caller);
        builder.field("contract", &self.0.contract);
        builder.finish()
    }
}

/// Wraps contracts::ContractExecutionEvent<Runtime> for Display impl and decodes contract events.
struct DisplayContractExecution<'a> {
    event: contracts::ContractExecutionEvent<Runtime>,
    transcoder: &'a Transcoder,
}

impl<'a> Display for DisplayContractExecution<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut builder = f.debug_struct("");
        builder.field("caller", &self.event.caller);
        match self
            .transcoder
            .decode_contract_event(&mut &self.event.data[..])
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

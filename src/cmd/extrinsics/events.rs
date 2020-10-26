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

use colored::Colorize;
use subxt::{
    contracts::*, Event, system::*, ContractsTemplateRuntime as Runtime,
    ExtrinsicSuccess, RawEvent
};
use super::{Transcoder, pretty_print};

pub fn display_events(result: &ExtrinsicSuccess<Runtime>, transcoder: &Transcoder) {
    for event in &result.events {
        print!(
            "{}::{} ",
            event.module.bold(),
            event.variant.bright_cyan().bold(),
        );

        if display_matching_event(event, |e: ExtrinsicSuccessEvent<Runtime>| e) {
            continue;
        }
        if display_matching_event(event, |e: ExtrinsicFailedEvent<Runtime>| e) {
            continue;
        }
        if display_matching_event(event, |e: NewAccountEvent<Runtime>| e) {
            continue;
        }
        if display_matching_event(event, |event: ContractExecutionEvent<Runtime>| DisplayContractExecution { transcoder, event } ) {
            continue;
        }
        println!();
        log::info!("{}::{} event has no matching custom display", event.module, event.variant);
    }
}

/// Prints the details for the given event if it matches.
///
/// Returns true iff the module and event name match.
fn display_matching_event<E, F, D>(raw_event: &RawEvent, new_display: F) -> bool
where
    E: Event<Runtime>,
    F: FnOnce(E) -> D,
    D: DisplayEvent,
{
    if raw_event.module != E::MODULE || raw_event.variant != E::EVENT {
        return false
    }

    match E::decode(&mut &raw_event.data[..]) {
        Ok(event) => {
            let display_event = new_display(event);
            display_event.print();
        },
        Err(err) => {
            print!(
                "{} {}",
                "Error decoding event:".bright_red().bold(),
                format!("{}", err),
            );
        }
    }
    println!();
    true
}

trait DisplayEvent {
    fn print(&self);
}

impl DisplayEvent for ExtrinsicSuccessEvent<Runtime> {
    fn print(&self) {
    }
}

impl DisplayEvent for ExtrinsicFailedEvent<Runtime> {
    fn print(&self) {
        println!(
            "{}",
            format!("{:?}", self.error).bright_red().bold()
        )
    }
}

impl DisplayEvent for NewAccountEvent<Runtime> {
    fn print(&self) {
        println!(
            "account: {}",
            format!("{}", self.account).bold()
        )
    }
}

struct DisplayContractExecution<'a> {
    event: ContractExecutionEvent<Runtime>,
    transcoder: &'a Transcoder,
}

impl<'a> DisplayEvent for DisplayContractExecution<'a> {
    fn print(&self) {
        match self.transcoder.decode_events(&mut &self.event.data[..]) {
            Ok(events) => {
                let _ = pretty_print(events);
            },
            Err(err) => {
                println!(
                    "{} {}",
                    "Error decoding contract event:".bright_red().bold(),
                    format!("{}", err),
                );
            }
        }
    }
}

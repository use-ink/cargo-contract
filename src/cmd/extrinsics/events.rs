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

use anyhow::Result;
use colored::Colorize;
use subxt::{
    balances::Balances, contracts::*, Event, system::*, ClientBuilder, ContractsTemplateRuntime,
    ExtrinsicSuccess, Signer, RawEvent
};
use std::fmt::{self, Formatter};

#[derive(Debug)]
pub struct DecodedEvent<E: Event<ContractsTemplateRuntime>> {
    event: E,
    module_name: &'static str,
    event_name: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct EventKey {
    module_name: &'static str,
    event_name: &'static str,
}

trait RuntimeEvent = Event<ContractsTemplateRuntime>;
type XtSuccess = ExtrinsicSuccessEvent<ContractsTemplateRuntime>;
type XtFailed = Extrinsic<ContractsTemplateRuntime>;

pub fn display_events(result: &ExtrinsicSuccess<ContractsTemplateRuntime>) {
    fn decode_or_error<E, F>(raw_event: RawEvent, f: F)
    where
        F: FnOnce(E),
        E: RuntimeEvent
    {
        match E::decode(&mut &event.data[..]) {
            Ok(event) => f(event),
            Err(err) => {
                println!("{}: {}", "Error decoding event")
            }
        }
    }

    for event in result.events {
        match (event.module, event.variant) {
            // System::ExtrinsicSuccess
            (<XtSuccess as RuntimeEvent>::MODULE, <XtSuccess as RuntimeEvent>::EVENT) => {
                println!(
                    "{}::{}",
                    xt_success.module_name.bold(),
                    xt_success.event_name.bright_cyan().bold()
                );
            },
            // System::ExtrinsicFailed
            (<XtFailed as RuntimeEvent>::MODULE, <XtFailed as RuntimeEvent>::EVENT) => {
                let decoded = XtFailed::decode(&mut &event.data[..])
                println!(
                    "{}::{}",
                    xt_failed.module_name.bold(),
                    xt_failed.event_name.bright_red().bold()
                );
                println!(
                    "  {}",
                    format!("{:?}", xt_failed.event.error).bright_red().bold()
                );
            },
            (module, module) => {
                println!(
                    "{}::{}",
                    module.bold(),
                    module.bright_cyan().bold()
                );
            }
        }
    }
}

trait DisplayEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result;
}

impl DisplayEvent for ExtrinsicSuccessEvent<ContractsTemplateRuntime> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}::{}",
            xt_success.module_name.bold(),
            xt_success.event_name.bright_cyan().bold()
        )
    }
}

/// Find the Event for the given module/variant, attempting to decode the event data.
pub fn find_event<E>(
    result: &ExtrinsicSuccess<ContractsTemplateRuntime>,
) -> Result<Option<DecodedEvent<E>>>
    where
        E: Event<ContractsTemplateRuntime>,
{
    if let Some(event) = result.find_event_raw(E::MODULE, E::EVENT) {
        let event = DecodedEvent {
            event: E::decode(&mut &event.data[..])?,
            module_name: E::MODULE,
            event_name: E::EVENT,
        };
        Ok(Some(event))
    } else {
        Ok(None)
    }
}

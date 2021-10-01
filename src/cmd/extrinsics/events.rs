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
    runtime_api::{api::contracts::events::ContractEmitted, ContractsRuntime},
    transcode::{env_types, ContractMessageTranscoder, TranscoderBuilder, Value},
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

        let display_event =
            // decode the contract event if it is `ContractEmitted`
            if <ContractEmitted as Event>::is_event(&event.pallet, &event.variant) {
                if let Value::Map(map) = decoded_event {
                    let fields_with_decoded_contract_event = map
                        .iter()
                        .map(|(key, value)| {
                            if key == &Value::String("data".into()) {
                                if let Value::Bytes(bytes) = value {
                                    let contract_event = transcoder.decode_contract_event(&mut bytes.bytes())?;
                                    Ok((key.clone(), contract_event))
                                } else {
                                    Err(anyhow::anyhow!("ContractEmitted::data should be `Vec<u8>`"))
                                }
                            } else {
                                Ok((key.clone(), value.clone()))
                            }
                        })
                        .collect::<Result<_>>()?;
                    Ok(Value::Map(fields_with_decoded_contract_event))
                } else {
                    // todo: [AJ] possibly handle legacy tuple struct for older version of contracts pallet?
                    Err(anyhow::anyhow!("ContractEmitted should be a struct with named fields"))
                }
            } else {
                Ok(decoded_event)
            }?;

        pretty_print(display_event, true)?;
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

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
    transcode::{env_types, ContractMessageTranscoder, TranscoderBuilder},
};
use crate::Verbosity;

use anyhow::Result;
use codec::Input;
use subxt::{self, DefaultConfig, Event, TransactionEvents};

pub fn display_events(
    result: &TransactionEvents<DefaultConfig>,
    transcoder: &ContractMessageTranscoder,
    subxt_metadata: &subxt::Metadata,
    verbosity: &Verbosity,
) -> Result<()> {
    if matches!(verbosity, Verbosity::Quiet) {
        return Ok(());
    }

    let runtime_metadata = subxt_metadata.runtime_metadata();
    let events_transcoder = TranscoderBuilder::new(&runtime_metadata.types)
        .register_custom_type::<sp_runtime::AccountId32, _>(Some("AccountId"), env_types::AccountId)
        .register_custom_type::<sp_runtime::AccountId32, _>(None, env_types::AccountId)
        .done();

    for event in result.as_slice() {
        log::debug!("displaying event {}::{}", event.pallet, event.variant);

        let event_metadata = subxt_metadata.event(event.pallet_index, event.variant_index)?;
        let event_fields = event_metadata.variant().fields();

        // todo: print event fields per line indented, possibly display only fields we are interested in...

        println!("Event: {} {}", event.pallet, event.variant);
        let event_data = &mut &event.data[..];
        for field in event_fields {
            if <ContractEmitted as Event>::is_event(&event.pallet, &event.variant)
                && field.name() == Some(&"data".to_string())
            {
                // data is a byte vec so the first byte is the length.
                let _data_len = event_data.read_byte()?;
                let contract_event = transcoder.decode_contract_event(event_data)?;
                println!("Event: {}", contract_event);
            } else {
                if let Some(name) = field.name() {
                    print!("{}: ", name);
                }
                let decoded_field = events_transcoder.decode(field, event_data)?;
                println!("{}", decoded_field)
            }
        }

        println!();
    }
    println!();
    Ok(())
}

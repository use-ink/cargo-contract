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

pub mod call;
pub mod deploy;
pub mod instantiate;
mod transcode;

use anyhow::Result;
use bat::PrettyPrinter;
use std::{fmt::Debug, fs::File};
use subxt::{ContractsTemplateRuntime, Event, ExtrinsicSuccess};

use self::transcode::Transcoder;
use crate::{crate_metadata::CrateMetadata, workspace::ManifestPath};

pub fn load_metadata() -> Result<ink_metadata::InkProject> {
    let manifest_path = ManifestPath::default();
    // todo: add metadata path option
    let metadata_path: Option<std::path::PathBuf> = None;
    let path = match metadata_path {
        Some(path) => path,
        None => {
            let crate_metadata = CrateMetadata::collect(&manifest_path)?;
            crate_metadata.metadata_path()
        }
    };
    let metadata = serde_json::from_reader(File::open(path)?)?;
    Ok(metadata)
}

#[derive(Debug)]
pub struct DecodedEvent<E: Event<ContractsTemplateRuntime>> {
    event: E,
    module_name: &'static str,
    event_name: &'static str,
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

pub fn pretty_print<V>(value: V) -> Result<()>
where
    V: Debug,
{
    let content = format!("{:#?}", value);
    let mut pretty_printer = PrettyPrinter::new();
    pretty_printer
        .input_from_bytes(content.as_bytes())
        .language("rust")
        .tab_width(Some(4))
        .true_color(false)
        .header(false)
        .line_numbers(false)
        .grid(false);
    let _ = pretty_printer.print();
    Ok(())
}

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
mod events;
pub mod instantiate;
pub mod instantiate_with_code;
mod runtime_api;
mod transcode;

use anyhow::Result;
use bat::PrettyPrinter;
use std::{fmt::Display, fs::File};

use self::{events::display_events, transcode::ContractMessageTranscoder};
use crate::{crate_metadata::CrateMetadata, workspace::ManifestPath};
use sp_core::sr25519;
use subxt::PairSigner;

type Balance = u128;

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

pub fn pretty_print<V>(value: V, indentation: bool) -> Result<()>
where
    V: Display,
{
    let content = if indentation {
        format!("{:#}", value)
    } else {
        format!("{}", value)
    };
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

pub fn pair_signer(
    pair: sr25519::Pair,
) -> PairSigner<runtime_api::ContractsRuntime, sr25519::Pair> {
    PairSigner::new(pair)
}

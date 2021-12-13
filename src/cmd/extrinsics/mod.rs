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
mod runtime_api;
mod transcode;

use anyhow::{anyhow, Context, Result};
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
    let metadata_path =
        File::open(&path).context(format!("Failed to open metadata file {}", path.display()))?;
    let metadata: contract_metadata::ContractMetadata = serde_json::from_reader(metadata_path)
        .context(format!(
            "Failed to deserialize metadata file {}",
            path.display()
        ))?;
    let ink_metadata =
        serde_json::from_value(serde_json::Value::Object(metadata.abi)).context(format!(
            "Failed to deserialize ink project metadata from file {}",
            path.display()
        ))?;
    if let ink_metadata::MetadataVersioned::V1(ink_project) = ink_metadata {
        Ok(ink_project)
    } else {
        Err(anyhow!("Unsupported ink metadata version. Expected V1"))
    }
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
) -> PairSigner<runtime_api::api::DefaultConfig, sr25519::Pair> {
    PairSigner::new(pair)
}

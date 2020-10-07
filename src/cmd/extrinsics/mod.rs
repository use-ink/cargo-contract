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
mod codec;
pub mod deploy;
pub mod instantiate;

use crate::{crate_metadata::CrateMetadata, workspace::ManifestPath};
use anyhow::Result;
use std::fs::File;

use self::codec::Codec;

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



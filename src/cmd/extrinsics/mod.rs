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

mod call;
mod events;
mod instantiate;
mod runtime_api;
mod transcode;
mod upload;

use anyhow::{anyhow, Context, Result};
use bat::PrettyPrinter;
use std::{fmt::Display, fs::File, path::PathBuf};

use self::{events::display_events, transcode::ContractMessageTranscoder};
use crate::{crate_metadata::CrateMetadata, workspace::ManifestPath};
use sp_core::sr25519;
use subxt::{Config, DefaultConfig};

pub use call::CallCommand;
pub use instantiate::InstantiateCommand;
pub use upload::UploadCommand;

type Balance = u128;
type CodeHash = <DefaultConfig as Config>::Hash;
type ContractAccount = <DefaultConfig as Config>::AccountId;
type PairSigner = subxt::PairSigner<DefaultConfig, SignedExtra, sp_core::sr25519::Pair>;
type SignedExtra = subxt::DefaultExtra<DefaultConfig>;
type RuntimeApi = runtime_api::api::RuntimeApi<DefaultConfig, SignedExtra>;

pub fn load_metadata(manifest_path: Option<&PathBuf>) -> Result<ink_metadata::InkProject> {
    let manifest_path = ManifestPath::try_from(manifest_path.as_ref())?;
    let crate_metadata = CrateMetadata::collect(&manifest_path)?;
    let path = crate_metadata.metadata_path();

    let file =
        File::open(&path).context(format!("Failed to open metadata file {}", path.display()))?;
    let metadata: contract_metadata::ContractMetadata = serde_json::from_reader(file).context(
        format!("Failed to deserialize metadata file {}", path.display()),
    )?;
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

pub fn pair_signer(pair: sr25519::Pair) -> PairSigner {
    PairSigner::new(pair)
}

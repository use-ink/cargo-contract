// Copyright 2018-2023 Parity Technologies (UK) Ltd.
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

use std::path::PathBuf;

use crate::{
    cmd::extrinsics::find_contract_artifacts,
    DEFAULT_KEY_COL_WIDTH,
};
use anyhow::Result;
use colored::Colorize as _;

#[derive(Debug, Clone, clap::Args)]
#[clap(
    name = "encode",
    about = "Encodes a contracts input calls and their arguments"
)]
pub struct EncodeCommand {
    /// Path to a contract build artifact file: a raw `.wasm` file, a `.contract` bundle,
    /// or a `.json` metadata file.
    #[clap(value_parser, conflicts_with = "manifest_path")]
    file: Option<PathBuf>,
    /// Path to the `Cargo.toml` of the contract.
    #[clap(long, value_parser)]
    manifest_path: Option<PathBuf>,
    /// The name of the contract message to encode.
    #[clap(long, short)]
    message: String,
    /// The arguments to encode
    #[clap(long, num_args = 0..)]
    args: Vec<String>,
}

impl EncodeCommand {
    pub fn run(&self) -> Result<()> {
        let artifacts =
            find_contract_artifacts(self.manifest_path.as_ref(), self.file.as_ref())?;
        let transcoder = artifacts.contract_transcoder()?;

        let call_data = transcoder.encode(&self.message, &self.args)?;
        let call_data_encoded = hex::encode_upper(call_data);

        println!(
            "{:>width$} {}",
            "Encoded data:".bright_green().bold(),
            call_data_encoded,
            width = DEFAULT_KEY_COL_WIDTH
        );

        Ok(())
    }
}

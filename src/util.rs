// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of ink!.
//
// ink! is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// ink! is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with ink!.  If not, see <http://www.gnu.org/licenses/>.

use anyhow::{Context, Result};
use cargo_metadata::{Metadata as CargoMetadata, MetadataCommand};
use rustc_version::Channel;
use std::path::PathBuf;

/// Get the result of `cargo metadata`
pub fn get_cargo_metadata(working_dir: Option<&PathBuf>) -> Result<CargoMetadata> {
	let mut cmd = MetadataCommand::new();
	if let Some(dir) = working_dir {
		cmd.current_dir(dir);
	}
	cmd.exec().context("Error invoking `cargo metadata`")
}

/// Check whether the current rust channel is valid: `nightly` is recommended.
pub fn assert_channel() -> Result<()> {
	let meta = rustc_version::version_meta()?;
	match meta.channel {
		Channel::Dev | Channel::Nightly => Ok(()),
		Channel::Stable | Channel::Beta => {
			anyhow::bail!(
				"cargo-contract cannot build using the {:?} channel. \
				 Switch to nightly.",
				meta.channel
			);
		}
	 }
}

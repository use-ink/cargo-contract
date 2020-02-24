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

use crate::manifest::ManifestPath;
use anyhow::{Context, Result};
use cargo_metadata::{Metadata as CargoMetadata, MetadataCommand, PackageId};
use rustc_version::Channel;
use std::{ffi::OsStr, process::Command};

/// Get the result of `cargo metadata`, together with the root package id.
pub fn get_cargo_metadata(manifest_path: &ManifestPath) -> Result<(CargoMetadata, PackageId)> {
    let mut cmd = MetadataCommand::new();
    let metadata = cmd
        .manifest_path(manifest_path)
        .exec()
        .context("Error invoking `cargo metadata`")?;
    let root_package_id = metadata
        .resolve
        .as_ref()
        .and_then(|resolve| resolve.root.as_ref())
        .context("Cannot infer the root project id")?
        .clone();
    Ok((metadata, root_package_id))
}

/// Check whether the current rust channel is valid: `nightly` is recommended.
pub fn assert_channel() -> Result<()> {
    let meta = rustc_version::version_meta()?;
    match meta.channel {
        Channel::Dev | Channel::Nightly => Ok(()),
        Channel::Stable | Channel::Beta => {
            anyhow::bail!(
                "cargo-contract cannot build using the {:?} channel. \
				 Switch to nightly. \
				 See https://github.com/paritytech/cargo-contract/tree/aj-xargo#build-requires-the-nightly-toolchain",
                format!("{:?}", meta.channel).to_lowercase(),
            );
        }
    }
}

/// Run cargo with the supplied args
pub(crate) fn invoke_cargo<I, S>(command: &str, args: I) -> Result<()>
where
    I: IntoIterator<Item = S> + std::fmt::Debug,
    S: AsRef<OsStr>,
{
    let cargo = std::env::var("CARGO").unwrap_or("cargo".to_string());
    let mut cmd = Command::new(cargo);
    cmd.arg(command);
    cmd.args(args);

    let status = cmd
        .status()
        .context(format!("Error executing `{:?}`", cmd))?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("`{:?}` failed with exit code: {:?}", cmd, status.code());
    }
}

#[cfg(test)]
pub mod tests {
    use std::path::PathBuf;
    use tempfile::TempDir;

    pub fn with_tmp_dir<F: FnOnce(&PathBuf)>(f: F) {
        let tmp_dir = TempDir::new().expect("temporary directory creation failed");

        f(&tmp_dir.into_path());
    }
}

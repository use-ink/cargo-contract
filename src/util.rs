// Copyright 2018-2021 Parity Technologies (UK) Ltd.
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

use crate::Verbosity;
use anyhow::{Context, Result};
use rustc_version::Channel;
use std::path::PathBuf;
use std::{ffi::OsStr, path::Path, process::Command};

/// Check whether the current rust channel is valid: `nightly` is recommended.
pub fn assert_channel() -> Result<()> {
    let meta = rustc_version::version_meta()?;
    match meta.channel {
        Channel::Dev | Channel::Nightly => Ok(()),
        Channel::Stable | Channel::Beta => {
            anyhow::bail!(
                "cargo-contract cannot build using the {:?} channel. \
                Switch to nightly. \
                See https://github.com/paritytech/cargo-contract#build-requires-the-nightly-toolchain",
                format!("{:?}", meta.channel).to_lowercase(),
            );
        }
    }
}

/// Run cargo with the supplied args
///
/// If successful, returns the stdout bytes
pub(crate) fn invoke_cargo<I, S, P>(
    command: &str,
    args: I,
    working_dir: Option<P>,
    verbosity: Option<Verbosity>,
) -> Result<Vec<u8>>
where
    I: IntoIterator<Item = S> + std::fmt::Debug,
    S: AsRef<OsStr>,
    P: AsRef<Path>,
{
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = Command::new(cargo);
    if let Some(path) = working_dir {
        log::debug!("Setting cargo working dir to '{}'", path.as_ref().display());
        cmd.current_dir(path);
    }

    cmd.arg(command);
    cmd.args(args);
    match verbosity {
        Some(Verbosity::Quiet) => cmd.arg("--quiet"),
        Some(Verbosity::Verbose) => cmd.arg("--verbose"),
        None => &mut cmd,
    };

    log::info!("invoking cargo: {:?}", cmd);

    let child = cmd
        // capture the stdout to return from this function as bytes
        .stdout(std::process::Stdio::piped())
        .spawn()
        .context(format!("Error executing `{:?}`", cmd))?;
    let output = child.wait_with_output()?;

    if output.status.success() {
        Ok(output.stdout)
    } else {
        anyhow::bail!(
            "`{:?}` failed with exit code: {:?}",
            cmd,
            output.status.code()
        );
    }
}

/// Returns the base name of the path.
pub(crate) fn base_name(path: &PathBuf) -> &str {
    path.file_name()
        .expect("file name must exist")
        .to_str()
        .expect("must be valid utf-8")
}

#[cfg(test)]
pub mod tests {
    use std::path::Path;

    pub fn with_tmp_dir<F>(f: F)
    where
        F: FnOnce(&Path) -> anyhow::Result<()>,
    {
        let tmp_dir = tempfile::Builder::new()
            .prefix("cargo-contract.test.")
            .tempdir()
            .expect("temporary directory creation failed");

        // catch test panics in order to clean up temp dir which will be very large
        f(tmp_dir.path()).expect("Error executing test with tmp dir")
    }
}

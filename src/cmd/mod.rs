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
use std::{
    io::{self, Write},
    path::PathBuf,
    process::Command,
};

mod build;
#[cfg(feature = "extrinsics")]
mod deploy;
#[cfg(feature = "extrinsics")]
mod extrinsics;
#[cfg(feature = "extrinsics")]
mod instantiate;
mod metadata;
mod new;

pub(crate) use self::{
    build::execute_build, metadata::execute_generate_metadata, new::execute_new,
};
#[cfg(feature = "extrinsics")]
pub(crate) use self::{
    deploy::execute_deploy, extrinsics::submit_extrinsic, instantiate::execute_instantiate,
};

/// Get the result of `cargo metadata`
pub(crate) fn get_cargo_metadata(working_dir: Option<&PathBuf>) -> Result<CargoMetadata> {
    let mut cmd = MetadataCommand::new();
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }
    cmd.exec().context("Error invoking `cargo metadata`")
}

/// Run the given command in the rustup nightly environment
pub(crate) fn rustup_run(
    command: &str,
    subcommand: &str,
    args: &[&'static str],
    working_dir: Option<&PathBuf>,
) -> Result<()> {
    if which::which("rustup").is_err() {
        anyhow::bail!(
            "The 'rustup' tool not was not found. \
             See: https://github.com/rust-lang/rustup#installation"
        )
    }

    let mut cmd = Command::new("rustup");

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    let output = cmd
        .arg("run")
        .arg("nightly")
        .arg(command)
        .arg(subcommand)
        .args(args)
        .output()?;

    if !output.status.success() {
        // Dump the output streams produced by cargo into the stdout/stderr.
        io::stdout().write_all(&output.stdout)?;
        io::stderr().write_all(&output.stderr)?;
        anyhow::bail!("{} {} failed", command, subcommand);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use tempfile::TempDir;

    pub fn with_tmp_dir<F: FnOnce(&PathBuf)>(f: F) {
        let tmp_dir = TempDir::new().expect("temporary directory creation failed");

        f(&tmp_dir.into_path());
    }
}

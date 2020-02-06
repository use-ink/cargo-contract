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

use anyhow::Result;
use std::{
    io::{self, Write},
    path::PathBuf,
    process::Command,
};

pub(crate) fn exec_cargo(
    command: &str,
    args: &[&'static str],
    working_dir: Option<&PathBuf>,
) -> Result<()> {
    let mut cmd = Command::new("cargo");
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    if !is_nightly(working_dir)? {
        cmd.arg("+nightly");
    }

    let output = cmd.arg(command).args(args).output()?;

    if !output.status.success() {
        // Dump the output streams produced by cargo into the stdout/stderr.
        io::stdout().write_all(&output.stdout)?;
        io::stderr().write_all(&output.stderr)?;
        anyhow::bail!("Build failed");
    }

    Ok(())
}

pub(crate) fn is_nightly(working_dir: Option<&PathBuf>) -> Result<bool> {
    let mut cmd = Command::new("cargo");
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }
    let output = cmd.arg("--version").output()?;
    let decoded = String::from_utf8(output.stdout).unwrap_or_default();
    Ok(decoded.contains("-nightly"))
}

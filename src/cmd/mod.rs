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

use crate::ExtrinsicOpts;
use anyhow::Result;
use std::{
    io::{self, Write},
    path::PathBuf,
    process::Command,
};
use subxt::{ClientBuilder, DefaultNodeRuntime, ExtrinsicSuccess};

mod build;
#[cfg(feature = "extrinsics")]
mod deploy;
#[cfg(feature = "extrinsics")]
mod instantiate;
mod metadata;
mod new;

#[cfg(feature = "extrinsics")]
pub(crate) use self::deploy::execute_deploy;
#[cfg(feature = "extrinsics")]
pub(crate) use self::instantiate::execute_instantiate;
pub(crate) use self::{
    build::execute_build, metadata::execute_generate_metadata, new::execute_new,
};

fn exec_cargo(command: &str, args: &[&'static str], working_dir: Option<&PathBuf>) -> Result<()> {
    let mut cmd = Command::new("cargo");
    let mut is_nightly_cmd = Command::new("cargo");
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
        is_nightly_cmd.current_dir(dir);
    }

    let is_nightly_default = is_nightly_cmd
        .arg("--version")
        .output()
        .map_err(|_| ())
        .and_then(|o| String::from_utf8(o.stdout).map_err(|_| ()))
        .unwrap_or_default()
        .contains("-nightly");

    if !is_nightly_default {
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

/// Submits an extrinsic to a substrate node, waits for it to succeed and returns an event expected
/// to have been triggered by the extrinsic.
fn submit_extrinsic<C, E>(
    extrinsic_opts: &ExtrinsicOpts,
    call: subxt::Call<C>,
    event_mod: &str,
    event_name: &str,
) -> Result<E>
where
    C: codec::Encode,
    E: codec::Decode,
{
    let result: Result<ExtrinsicSuccess<_>> = async_std::task::block_on(async move {
        let cli = ClientBuilder::<DefaultNodeRuntime>::new()
            .set_url(&extrinsic_opts.url.to_string())
            .build()
            .await?;
        let signer = extrinsic_opts.signer()?;
        let xt = cli.xt(signer, None).await?;
        let success = xt.watch().submit(call).await?;
        Ok(success)
    });

    match result?.find_event::<E>(event_mod, event_name) {
        Some(Ok(hash)) => Ok(hash),
        Some(Err(err)) => Err(anyhow::anyhow!(
            "Failed to decode event '{} {}': {}",
            event_mod,
            event_name,
            err
        )),
        None => Err(anyhow::anyhow!(
            "Failed to find '{} {}' Event",
            event_mod,
            event_name
        )),
    }
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

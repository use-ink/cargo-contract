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

use std::{
    fs,
    path::PathBuf,
};
use anyhow::{Context, Result};
use toml::value;

/// Executes build of the smart-contract which produces a wasm binary that is ready for deploying.
///
/// It does so by invoking build by cargo and then post processing the final binary.
pub(crate) fn execute_generate_metadata(dir: Option<&PathBuf>) -> Result<String> {
    println!("  Generating metadata");

    // check for existing .Cargo.toml.bk?
    // todo: [AJ] check for rlib enabled, add if not
    // - copy Cargo.toml to .Cargo.toml.bk or whatever (and ignore in gitignore)
    // - add rlib crate type
    // - exec build
    // - rename backup to original

    let cargo_metadata = super::get_cargo_metadata(dir)?;

    with_contract_rust_lib(&cargo_metadata, || {
        super::rustup_run(
            "cargo",
            "run",
            &[
                "--package",
                "abi-gen",
                "--release",
                // "--no-default-features", // Breaks builds for MacOS (linker errors), we should investigate this issue asap!
                "--verbose",
            ],
            dir,
        )
    });

    let mut out_path = cargo_metadata.target_directory;
    out_path.push("metadata.json");

    Ok(format!(
        "Your metadata file is ready.\nYou can find it here:\n{}",
        out_path.display()
    ))
}

/// Adds the 'rlib' crate_type to the Cargo.toml if not present.
/// Makes a backup of the existing Cargo.toml which is restored once complete.
fn with_contract_rust_lib<F: FnOnce() -> Result<()>>(cargo_meta: &cargo_metadata::Metadata, f: F) -> Result<()> {
    let cargo_toml = cargo_meta.workspace_root.join("Cargo.toml");
    let backup = cargo_meta.workspace_root.join(".Cargo.toml.bk");

    // todo: acquire workspace lock here before doing all this

    let toml = fs::read_to_string(&cargo_toml)?;
    let mut toml: value::Table = toml::from_str(&toml)?;
    let mut crate_types = toml.get_mut("lib")
        .and_then(|v| v.try_into::<value::Table>().ok())
        .and_then(|mut t| t.get_mut("crate-type"))
        .and_then(|v| v.try_into::<value::Array>().ok())
        .ok_or(anyhow::anyhow!("No [lib] crate-type section found"))?;

    if crate_types.iter().any(|v| v.as_str().map_or(false, |s| s == "rlib")) {
        log::debug!("rlib crate-type already specified in Cargo.toml");
        return f()
    }

    fs::copy(&cargo_toml, &backup).context("Creating a backup for Cargo.toml")?;

    // add rlib to crate-types and write updated Cargo.toml
    crate_types.push(value::Value::String("rlib".into()));

    let updated_toml = toml::to_string(&toml)?;
    fs::write(&cargo_toml, updated_toml).context("Writing updated Cargo.toml")?;

    // Now run the function with a modified Cargo.toml in place
    let result = f();

    fs::rename(&backup, &cargo_toml).context("Restoring the backup of Cargo.toml")?;
    result
}

#[cfg(feature = "test-ci-only")]
#[cfg(test)]
mod tests {
    use crate::cmd::{execute_generate_metadata, execute_new, tests::with_tmp_dir};

    #[test]
    fn generate_metadata() {
        with_tmp_dir(|path| {
            execute_new("new_project", Some(path)).expect("new project creation failed");
            let working_dir = path.join("new_project");
            execute_generate_metadata(Some(&working_dir)).expect("generate metadata failed");

            let mut abi_file = working_dir;
            abi_file.push("target");
            abi_file.push("metadata.json");
            assert!(abi_file.exists())
        });
    }
}

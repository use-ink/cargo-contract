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
use colored::Colorize;
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};
use toml::value;

/// Load and temporarily modify the manifest file (Cargo.toml).
pub struct CargoToml {
    path: PathBuf,
}

impl CargoToml {
    /// Create new CargoToml for the given manifest path.
    ///
    /// The path *must* be to a `Cargo.toml`.
    pub fn new(path: &PathBuf) -> Result<CargoToml> {
        if let Some(file_name) = path.file_name() {
            if file_name != "Cargo.toml" {
                anyhow::bail!("Manifest file must be a Cargo.toml")
            }
        }

        Ok(CargoToml { path: path.clone() })
    }

    /// Amend the Cargo.toml and run the supplied function.
    /// Makes a backup of the existing Cargo.toml which is restored once complete.
    ///
    /// # Note
    ///
    /// If the program terminates while in progress then the amended `Cargo.toml` will remain in
    /// place. The user will be given the option to restore from the backup on the next run.
    ///
    /// # Arguments
    ///
    /// - `amend`: Accepts the mutable toml Table to modify, saving the result to the temporary
    /// `Cargo.toml`. If the manifest does not need to modified, should return false.
    /// - `f`: Function to be executed while the temporary amended `Cargo.toml` is in place. e.g.
    /// running a `cargo` command which will pick up the manifest.
    pub fn with_amended_manifest<A, F>(&self, amend: A, f: F) -> Result<()>
    where
        A: FnOnce(&mut value::Table) -> Result<bool>,
        F: FnOnce() -> Result<()>,
    {
        let mut backup_path = self.path.clone();
        backup_path.set_file_name(".Cargo.toml.bk");

        cleanup_existing_backup(&backup_path, &self.path)?;

        let toml = fs::read_to_string(&self.path)?;
        let mut table: value::Table = toml::from_str(&toml)?;

        // run supplied amend function
        let should_amend = amend(&mut table)?;

        if !should_amend {
            log::debug!("amend function returned false, so update not required");
            // Now run the function without a modified Cargo.toml
            return f();
        }

        fs::copy(&self.path, &backup_path).context("Creating a backup for Cargo.toml")?;

        let updated_toml = toml::to_string(&table)?;
        fs::write(&self.path, updated_toml).context("Writing updated Cargo.toml")?;

        // Now run the function with a modified Cargo.toml in place
        let result = f();

        fs::rename(&backup_path, &self.path).context("Restoring the backup of Cargo.toml")?;
        result
    }

    /// Amend the `[lib] crate-types = []` section
    fn with_amended_crate_types<A, F>(&self, amend: A, f: F) -> Result<()>
    where
        A: FnOnce(&mut value::Array) -> bool,
        F: FnOnce() -> Result<()>,
    {
        self.with_amended_manifest(
            |toml| {
                let lib = toml
                    .get_mut("lib")
                    .ok_or(anyhow::anyhow!("lib section not found"))?;
                let crate_types = lib
                    .get_mut("crate-type")
                    .ok_or(anyhow::anyhow!("crate-type section not found"))?;
                let crate_types = crate_types
                    .as_array_mut()
                    .ok_or(anyhow::anyhow!("crate-types should be an Array"))?;

                let should_amend = amend(crate_types);
                Ok(should_amend)
            },
            f,
        )
    }

    /// Add an value to the `[lib] crate-types = []` section
    ///
    /// If the value already exists, does nothing.
    pub fn with_added_crate_type<F>(&self, crate_type: &str, f: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        self.with_amended_crate_types(
            |crate_types| {
                if crate_type_exists(crate_type, crate_types) {
                    false
                } else {
                    crate_types.push(crate_type.into());
                    true
                }
            },
            f,
        )
    }

    /// Remove a value from the `[lib] crate-types = []` section
    ///
    /// If the value does not exist, does nothing.
    pub fn with_removed_crate_type<F>(&self, crate_type: &str, f: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        self.with_amended_crate_types(
            |crate_types| {
                if crate_type_exists(crate_type, crate_types) {
                    crate_types.retain(|v| v.as_str().map_or(true, |s| s != crate_type));
                    true
                } else {
                    false
                }
            },
            f,
        )
    }
}

/// Handle the case where a previous build was terminated leaving the modified Cargo.toml
/// and its backup in place.
///
/// Asks the user whether to restore the backup.
fn cleanup_existing_backup(backup_path: &PathBuf, manifest_path: &PathBuf) -> Result<()> {
    if backup_path.exists() {
        print!(
            "A backup of the original 'Cargo.toml' exists from a previous build which may have \
             terminated prematurely. The current 'Cargo.toml' is likely to be a temporary \
             modified copy created by this build process. Would you like to restore from the \
             backup and overwrite the current 'Cargo.toml'? {}",
            "[Y/n]".bold()
        );
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        match input.chars().nth(0) {
            Some('n') | Some('N') => Ok(()),
            _ => {
                fs::rename(&backup_path, &manifest_path)
                    .context("Creating a backup for Cargo.toml")?;
                Ok(())
            }
        }
    } else {
        Ok(())
    }
}

fn crate_type_exists(crate_type: &str, crate_types: &value::Array) -> bool {
    crate_types
        .iter()
        .any(|v| v.as_str().map_or(false, |s| s == crate_type))
}

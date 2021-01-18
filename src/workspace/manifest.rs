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

use anyhow::{Context, Result};

use super::{metadata, Profile};
use std::convert::{TryFrom, TryInto};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use toml::value;

const MANIFEST_FILE: &str = "Cargo.toml";
const LEGACY_METADATA_PACKAGE_PATH: &str = ".ink/abi_gen";
const METADATA_PACKAGE_PATH: &str = ".ink/metadata_gen";

/// Path to a Cargo.toml file
#[derive(Clone, Debug)]
pub struct ManifestPath {
    path: PathBuf,
}

impl ManifestPath {
    /// Create a new ManifestPath, errors if not path to `Cargo.toml`
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let manifest = path.as_ref();
        if let Some(file_name) = manifest.file_name() {
            if file_name != MANIFEST_FILE {
                anyhow::bail!("Manifest file must be a Cargo.toml")
            }
        }
        Ok(ManifestPath {
            path: manifest.into(),
        })
    }

    /// Create an arg `--manifest-path=` for `cargo` command
    pub fn cargo_arg(&self) -> String {
        format!("--manifest-path={}", self.path.to_string_lossy())
    }

    /// The directory path of the manifest path.
    ///
    /// Returns `None` if the path is just the plain file name `Cargo.toml`
    pub fn directory(&self) -> Option<&Path> {
        let just_a_file_name =
            self.path.iter().collect::<Vec<_>>() == vec![Path::new(MANIFEST_FILE)];
        if !just_a_file_name {
            self.path.parent()
        } else {
            None
        }
    }
}

impl TryFrom<&PathBuf> for ManifestPath {
    type Error = anyhow::Error;

    fn try_from(value: &PathBuf) -> Result<Self, Self::Error> {
        ManifestPath::new(value)
    }
}

impl<P> TryFrom<Option<P>> for ManifestPath
where
    P: AsRef<Path>,
{
    type Error = anyhow::Error;

    fn try_from(value: Option<P>) -> Result<Self, Self::Error> {
        value.map_or(Ok(Default::default()), ManifestPath::new)
    }
}

impl Default for ManifestPath {
    fn default() -> ManifestPath {
        ManifestPath::new(MANIFEST_FILE).expect("it's a valid manifest file")
    }
}

impl AsRef<Path> for ManifestPath {
    fn as_ref(&self) -> &Path {
        self.path.as_ref()
    }
}

impl From<ManifestPath> for PathBuf {
    fn from(path: ManifestPath) -> Self {
        path.path
    }
}

/// Create, amend and save a copy of the specified `Cargo.toml`.
pub struct Manifest {
    path: ManifestPath,
    toml: value::Table,
    /// True if a metadata package should be generated for this manifest
    metadata_package: bool,
}

impl Manifest {
    /// Create new Manifest for the given manifest path.
    ///
    /// The path *must* be to a `Cargo.toml`.
    pub fn new<P>(path: P) -> Result<Manifest>
    where
        P: TryInto<ManifestPath, Error = anyhow::Error>,
    {
        let manifest_path = path.try_into()?;
        let toml = fs::read_to_string(&manifest_path).context("Loading Cargo.toml")?;
        let toml: value::Table = toml::from_str(&toml)?;

        Ok(Manifest {
            path: manifest_path,
            toml,
            metadata_package: false,
        })
    }

    /// Get the path of the manifest file
    pub(super) fn path(&self) -> &ManifestPath {
        &self.path
    }

    /// Get mutable reference to `[lib] crate-types = []` section
    fn get_crate_types_mut(&mut self) -> Result<&mut value::Array> {
        let lib = self
            .toml
            .get_mut("lib")
            .ok_or_else(|| anyhow::anyhow!("lib section not found"))?;
        let crate_types = lib
            .get_mut("crate-type")
            .ok_or_else(|| anyhow::anyhow!("crate-type section not found"))?;

        crate_types
            .as_array_mut()
            .ok_or_else(|| anyhow::anyhow!("crate-types should be an Array"))
    }

    /// Add a value to the `[lib] crate-types = []` section.
    ///
    /// If the value already exists, does nothing.
    pub fn with_added_crate_type(&mut self, crate_type: &str) -> Result<&mut Self> {
        let crate_types = self.get_crate_types_mut()?;
        if !crate_type_exists(crate_type, crate_types) {
            crate_types.push(crate_type.into());
        }
        Ok(self)
    }

    /// Set `[profile.release]` lto flag
    pub fn with_profile_release_lto(&mut self, enabled: bool) -> Result<&mut Self> {
        let lto = self
            .get_profile_release_table_mut()?
            .entry("lto")
            .or_insert(enabled.into());
        *lto = enabled.into();
        Ok(self)
    }

    /// Set preferred defaults for the `[profile.release]` section
    ///
    /// # Note
    ///
    /// Existing user defined settings for this section are preserved. Only if a setting is not
    /// defined is the preferred default set.
    pub fn with_profile_release_defaults(&mut self, defaults: Profile) -> Result<&mut Self> {
        let profile_release = self.get_profile_release_table_mut()?;
        defaults.merge(profile_release);
        Ok(self)
    }

    /// Get mutable reference to `[profile.release]` section
    fn get_profile_release_table_mut(&mut self) -> Result<&mut value::Table> {
        let profile = self
            .toml
            .entry("profile")
            .or_insert(value::Value::Table(Default::default()));
        let release = profile
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("profile should be a table"))?
            .entry("release")
            .or_insert(value::Value::Table(Default::default()));
        release
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("release should be a table"))
    }

    /// Remove a value from the `[lib] crate-types = []` section
    ///
    /// If the value does not exist, does nothing.
    pub fn with_removed_crate_type(&mut self, crate_type: &str) -> Result<&mut Self> {
        let crate_types = self.get_crate_types_mut()?;
        if crate_type_exists(crate_type, crate_types) {
            crate_types.retain(|v| v.as_str().map_or(true, |s| s != crate_type));
        }
        Ok(self)
    }

    /// Adds a metadata package to the manifest workspace for generating metadata
    pub fn with_metadata_package(&mut self) -> Result<&mut Self> {
        let workspace = self
            .toml
            .entry("workspace")
            .or_insert(value::Value::Table(Default::default()));
        let members = workspace
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("workspace should be a table"))?
            .entry("members")
            .or_insert(value::Value::Array(Default::default()))
            .as_array_mut()
            .ok_or_else(|| anyhow::anyhow!("members should be an array"))?;

        if members.contains(&LEGACY_METADATA_PACKAGE_PATH.into()) {
            // warn user if they have legacy metadata generation artifacts
            use colored::Colorize;
            println!(
                "{} {} {} {}",
                "warning:".yellow().bold(),
                "please remove".bold(),
                LEGACY_METADATA_PACKAGE_PATH.bold(),
                "from the `[workspace]` section in the `Cargo.toml`, \
                and delete that directory. These are now auto-generated."
                    .bold()
            );
        } else {
            members.push(METADATA_PACKAGE_PATH.into());
        }

        self.metadata_package = true;
        Ok(self)
    }

    /// Replace relative paths with absolute paths with the working directory.
    ///
    /// Enables the use of a temporary amended copy of the manifest.
    ///
    /// # Rewrites
    ///
    /// - `[lib]/path`
    /// - `[dependencies]`
    ///
    /// Dependencies with package names specified in `exclude_deps` will not be rewritten.
    pub(super) fn rewrite_relative_paths<I, S>(&mut self, exclude_deps: I) -> Result<&mut Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let abs_path = self.path.as_ref().canonicalize()?;
        let abs_dir = abs_path
            .parent()
            .expect("The manifest path is a file path so has a parent; qed");

        let to_absolute = |value_id: String, existing_path: &mut value::Value| -> Result<()> {
            let path_str = existing_path
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("{} should be a string", value_id))?;
            let path = PathBuf::from(path_str);
            if path.is_relative() {
                let lib_abs = abs_dir.join(path);
                log::debug!("Rewriting {} to '{}'", value_id, lib_abs.display());
                *existing_path = value::Value::String(lib_abs.to_string_lossy().into())
            }
            Ok(())
        };

        let rewrite_path = |table_value: &mut value::Value, table_section: &str, default: &str| {
            let table = table_value.as_table_mut().ok_or_else(|| {
                anyhow::anyhow!("'[{}]' section should be a table", table_section)
            })?;

            match table.get_mut("path") {
                Some(existing_path) => {
                    to_absolute(format!("[{}]/path", table_section), existing_path)
                }
                None => {
                    let default_path = PathBuf::from(default);
                    if !default_path.exists() {
                        anyhow::bail!(
                            "No path specified, and the default `{}` was not found",
                            default
                        )
                    }
                    let path = abs_dir.join(default_path);
                    log::debug!("Adding default path '{}'", path.display());
                    table.insert(
                        "path".into(),
                        value::Value::String(path.to_string_lossy().into()),
                    );
                    Ok(())
                }
            }
        };

        // Rewrite `[lib] path = /path/to/lib.rs`
        if let Some(lib) = self.toml.get_mut("lib") {
            rewrite_path(lib, "lib", "src/lib.rs")?;
        }

        // Rewrite `[[bin]] path = /path/to/main.rs`
        if let Some(bin) = self.toml.get_mut("bin") {
            let bins = bin
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("'[[bin]]' section should be a table array"))?;

            // Rewrite `[[bin]] path =` value to an absolute path.
            for bin in bins {
                rewrite_path(bin, "[bin]", "src/main.rs")?;
            }
        }

        // Rewrite any dependency relative paths
        if let Some(dependencies) = self.toml.get_mut("dependencies") {
            let exclude = exclude_deps
                .into_iter()
                .map(|s| s.as_ref().to_string())
                .collect::<HashSet<_>>();
            let table = dependencies
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("dependencies should be a table"))?;
            for (name, value) in table {
                let package_name = {
                    let package = value.get("package");
                    let package_name = package.and_then(|p| p.as_str()).unwrap_or(name);
                    package_name.to_string()
                };

                if !exclude.contains(&package_name) {
                    if let Some(dependency) = value.as_table_mut() {
                        if let Some(dep_path) = dependency.get_mut("path") {
                            to_absolute(format!("dependency {}", package_name), dep_path)?;
                        }
                    }
                }
            }
        }

        Ok(self)
    }

    /// Writes the amended manifest to the given path.
    pub fn write(&self, manifest_path: &ManifestPath) -> Result<()> {
        if let Some(dir) = manifest_path.directory() {
            fs::create_dir_all(dir).context(format!("Creating directory '{}'", dir.display()))?;
        }

        if self.metadata_package {
            let dir = if let Some(manifest_dir) = manifest_path.directory() {
                manifest_dir.join(METADATA_PACKAGE_PATH)
            } else {
                METADATA_PACKAGE_PATH.into()
            };

            fs::create_dir_all(&dir).context(format!("Creating directory '{}'", dir.display()))?;

            let contract_package_name = self
                .toml
                .get("package")
                .ok_or_else(|| anyhow::anyhow!("package section not found"))?
                .get("name")
                .ok_or_else(|| anyhow::anyhow!("[package] name field not found"))?
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("[package] name should be a string"))?;

            let ink_metadata = self
                .toml
                .get("dependencies")
                .ok_or_else(|| anyhow::anyhow!("[dependencies] section not found"))?
                .get("ink_metadata")
                .ok_or_else(|| anyhow::anyhow!("ink_metadata dependency not found"))?
                .as_table()
                .ok_or_else(|| anyhow::anyhow!("ink_metadata dependency should be a table"))?;

            metadata::generate_package(dir, contract_package_name, ink_metadata.clone())?;
        }

        let updated_toml = toml::to_string(&self.toml)?;
        log::debug!(
            "Writing updated manifest to '{}'",
            manifest_path.as_ref().display()
        );
        fs::write(manifest_path, updated_toml)?;
        Ok(())
    }
}

fn crate_type_exists(crate_type: &str, crate_types: &value::Array) -> bool {
    crate_types
        .iter()
        .any(|v| v.as_str().map_or(false, |s| s == crate_type))
}

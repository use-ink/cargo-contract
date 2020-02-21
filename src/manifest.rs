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
use cargo_metadata::{Metadata as CargoMetadata, Package, PackageId};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};
use toml::value;

const MANIFEST_FILE: &str = "Cargo.toml";

/// Create an amended copy of `Cargo.toml`.
///
/// Relative paths are rewritten to absolute paths.
pub struct Manifest {
    path: PathBuf,
    toml: value::Table,
}

impl Manifest {
    /// Create new CargoToml for the given manifest path.
    ///
    /// The path *must* be to a `Cargo.toml`.
    pub fn new(path: &PathBuf) -> Result<Manifest> {
        if let Some(file_name) = path.file_name() {
            if file_name != MANIFEST_FILE {
                anyhow::bail!("Manifest file must be a Cargo.toml")
            }
        }

        let toml = fs::read_to_string(&path).context("Loading Cargo.toml")?;
        let toml: value::Table = toml::from_str(&toml)?;

        Ok(Manifest {
            path: path.clone(),
            toml,
        })
    }

    /// Get mutable reference to `[lib] crate-types = []` section
    fn get_crate_types_mut(&mut self) -> Result<&mut value::Array> {
        let lib = self
            .toml
            .get_mut("lib")
            .ok_or(anyhow::anyhow!("lib section not found"))?;
        let crate_types = lib
            .get_mut("crate-type")
            .ok_or(anyhow::anyhow!("crate-type section not found"))?;

        crate_types
            .as_array_mut()
            .ok_or(anyhow::anyhow!("crate-types should be an Array"))
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
    fn rewrite_relative_paths<I, S>(&mut self, exclude_deps: I) -> Result<&mut Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let abs_path = self.path.canonicalize()?;
        let abs_dir = abs_path
            .parent()
            .expect("The manifest path is a file path so has a parent; qed");

        let to_absolute = |value_id: String, existing_path: &mut value::Value| -> Result<()> {
            let path_str = existing_path
                .as_str()
                .ok_or(anyhow::anyhow!("{} should be a string", value_id))?;
            let path = PathBuf::from(path_str);
            if path.is_relative() {
                let lib_abs = abs_dir.join(path);
                log::debug!("Rewriting {} to '{}'", value_id, lib_abs.display());
                *existing_path = value::Value::String(lib_abs.to_string_lossy().into())
            }
            Ok(())
        };

        let rewrite_path = |table_value: &mut value::Value, table_section: &str, default: &str| {
            let table = table_value.as_table_mut().ok_or(anyhow::anyhow!(
                "'[{}]' section should be a table",
                table_section
            ))?;

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
                    let path = default_path.to_string_lossy();
                    log::debug!("Adding default path '{}'", path);
                    table.insert("path".into(), value::Value::String(path.into()));
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
                .ok_or(anyhow::anyhow!("'[[bin]]' section should be a table array"))?;

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
                .ok_or(anyhow::anyhow!("dependencies should be a table"))?;
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
    ///
    /// # Errors
    ///
    /// If the path is not for a `Cargo.toml` file
    pub fn write<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf> {
        let manifest_path = path.as_ref();
        if manifest_path.file_name() != Some(OsStr::new(MANIFEST_FILE)) {
            anyhow::bail!(
                "{} should be a Cargo.toml file path",
                manifest_path.display()
            )
        }

        if let Some(dir) = manifest_path.parent() {
            fs::create_dir_all(&dir).context(format!("Creating directory '{}'", dir.display()))?;
        }

        let updated_toml = toml::to_string(&self.toml)?;
        fs::write(&manifest_path, updated_toml).context(format!(
            "Writing updated manifest to '{}'",
            manifest_path.display()
        ))?;
        Ok(manifest_path.into())
    }
}

fn crate_type_exists(crate_type: &str, crate_types: &value::Array) -> bool {
    crate_types
        .iter()
        .any(|v| v.as_str().map_or(false, |s| s == crate_type))
}

pub struct Workspace {
    workspace_root: PathBuf,
    root_package: PackageId,
    members: HashMap<PackageId, (Package, Manifest)>,
}

impl Workspace {
    /// Create a new Workspace from the supplied cargo metadata.
    pub fn new(metadata: &CargoMetadata, root_package: &PackageId) -> Result<Self> {
        let member_manifest = |package_id: &PackageId| -> Result<(PackageId, (Package, Manifest))> {
            let package = metadata
                .packages
                .iter()
                .find(|p| p.id == *package_id)
                .expect(&format!(
                    "Package '{}' is a member and should be in the packages list",
                    package_id
                ));
            let manifest = Manifest::new(&package.manifest_path)?;
            Ok((package_id.clone(), (package.clone(), manifest)))
        };

        let members = metadata
            .workspace_members
            .iter()
            .map(member_manifest)
            .collect::<Result<HashMap<_, _>>>()?;

        if !members.contains_key(root_package) {
            anyhow::bail!("The root package should be a workspace member")
        }

        Ok(Workspace {
            workspace_root: metadata.workspace_root.clone(),
            root_package: root_package.clone(),
            members,
        })
    }

    /// Get a mutable reference to the root package's manifest.
    ///
    /// # Note
    ///
    /// The root package is the current workspace package being built, not to be confused with
    /// the workspace root (where the top level workspace Cargo.toml is defined).
    pub fn root_package_manifest_mut(&mut self) -> &mut Manifest {
        self.members
            .get_mut(&self.root_package)
            .map(|(_, m)| m)
            .expect("The root package should be a workspace member")
    }

    /// Writes the amended manifests to the `target` directory, retaining the workspace directory
    /// structure, but only with the `Cargo.toml` files.
    ///
    /// Relative paths will be rewritten to absolute paths from the original workspace root, except
    /// intra-workspace relative dependency paths which will be preserved.
    ///
    /// Returns the paths of the new manifests.
    pub fn write<P: AsRef<Path>>(&mut self, target: P) -> Result<Vec<(PackageId, PathBuf)>> {
        let exclude_member_package_names = self
            .members
            .iter()
            .map(|(_, (p, _))| p.name.clone())
            .collect::<Vec<_>>();
        let mut new_manifest_paths = Vec::new();
        for (package_id, (package, manifest)) in self.members.iter_mut() {
            // replace the original workspace root with the temporary directory
            let mut new_path: PathBuf = target.as_ref().into();
            new_path.push(package.manifest_path.strip_prefix(&self.workspace_root)?);

            manifest.rewrite_relative_paths(&exclude_member_package_names)?;
            manifest.write(&new_path)?;

            new_manifest_paths.push((package_id.clone(), new_path.into()));
        }
        Ok(new_manifest_paths)
    }

    /// Copy the workspace with amended manifest files to a temporary directory, executing the
    /// supplied function with the root manifest path before the directory is cleaned up.
    pub fn using_temp<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&Path) -> Result<()>,
    {
        let tmp_dir = tempfile::Builder::new()
            .prefix(".cargo-contract_")
            .tempdir()?;
        log::debug!("Using temp workspace at '{}'", tmp_dir.path().display());
        let new_paths = self.write(&tmp_dir)?;
        let root_manifest_path = new_paths
            .iter()
            .find_map(|(pid, path)| {
                if *pid == self.root_package {
                    Some(path)
                } else {
                    None
                }
            })
            .expect("root package should be a member of the temp workspace");
        f(root_manifest_path)
    }
}

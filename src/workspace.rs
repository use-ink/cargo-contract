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
use std::convert::{TryFrom, TryInto};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use toml::value;

const MANIFEST_FILE: &str = "Cargo.toml";

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

/// Create, amend and save a copy of the specified `Cargo.toml`.
pub struct Manifest {
    path: ManifestPath,
    toml: value::Table,
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
            .ok_or(anyhow::anyhow!("profile should be a table"))?
            .entry("release")
            .or_insert(value::Value::Table(Default::default()));
        release
            .as_table_mut()
            .ok_or(anyhow::anyhow!("release should be a table"))
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
        let abs_path = self.path.as_ref().canonicalize()?;
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
    pub fn write(&self, path: &ManifestPath) -> Result<()> {
        let manifest_path = path.as_ref();

        if let Some(dir) = manifest_path.parent() {
            fs::create_dir_all(&dir).context(format!("Creating directory '{}'", dir.display()))?;
        }

        let updated_toml = toml::to_string(&self.toml)?;
        log::debug!("Writing updated manifest to '{}'", manifest_path.display());
        fs::write(&manifest_path, updated_toml)?;
        Ok(())
    }
}

fn crate_type_exists(crate_type: &str, crate_types: &value::Array) -> bool {
    crate_types
        .iter()
        .any(|v| v.as_str().map_or(false, |s| s == crate_type))
}

/// Make a copy of a cargo workspace, maintaining only the directory structure and manifest
/// files. Relative paths to source files and non-workspace dependencies are rewritten to absolute
/// paths to the original locations.
///
/// This allows custom amendments to be made to the manifest files without editing the originals
/// directly.
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

    /// Amend the root package manifest using the supplied function.
    ///
    /// # Note
    ///
    /// The root package is the current workspace package being built, not to be confused with
    /// the workspace root (where the top level workspace Cargo.toml is defined).
    pub fn with_root_package_manifest<F>(&mut self, f: F) -> Result<&mut Self>
    where
        F: FnOnce(&mut Manifest) -> Result<()>,
    {
        let root_package_manifest = self
            .members
            .get_mut(&self.root_package)
            .map(|(_, m)| m)
            .expect("The root package should be a workspace member");
        f(root_package_manifest)?;
        Ok(self)
    }

    /// Writes the amended manifests to the `target` directory, retaining the workspace directory
    /// structure, but only with the `Cargo.toml` files.
    ///
    /// Relative paths will be rewritten to absolute paths from the original workspace root, except
    /// intra-workspace relative dependency paths which will be preserved.
    ///
    /// Returns the paths of the new manifests.
    pub fn write<P: AsRef<Path>>(&mut self, target: P) -> Result<Vec<(PackageId, ManifestPath)>> {
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
            let new_manifest = ManifestPath::new(new_path)?;

            manifest.rewrite_relative_paths(&exclude_member_package_names)?;
            manifest.write(&new_manifest)?;

            new_manifest_paths.push((package_id.clone(), new_manifest));
        }
        Ok(new_manifest_paths)
    }

    /// Copy the workspace with amended manifest files to a temporary directory, executing the
    /// supplied function with the root manifest path before the directory is cleaned up.
    pub fn using_temp<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&ManifestPath) -> Result<()>,
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

/// Subset of cargo profile settings to configure defaults for building contracts
pub struct Profile {
    opt_level: Option<OptLevel>,
    lto: Lto,
    // `None` means use rustc default.
    codegen_units: Option<u32>,
    overflow_checks: bool,
    panic: PanicStrategy,
}

impl Profile {
    /// The preferred set of defaults for compiling a release build of a contract
    pub fn default_contract_release() -> Profile {
        Profile {
            opt_level: Some(OptLevel::Z),
            lto: Lto::Fat,
            codegen_units: Some(1),
            overflow_checks: true,
            panic: PanicStrategy::Abort,
        }
    }

    /// Set any unset profile settings from the config.
    ///
    /// Therefore:
    ///   - If the user has explicitly defined a profile setting, it will not be overwritten.
    ///   - If a profile setting is not defined, the value from this profile instance will be added
    fn merge(&self, profile: &mut value::Table) {
        let mut set_value_if_vacant = |key: &'static str, value: value::Value| {
            if !profile.contains_key(key) {
                profile.insert(key.into(), value);
            }
        };
        if let Some(opt_level) = self.opt_level {
            set_value_if_vacant("opt-level", opt_level.to_toml_value());
        }
        set_value_if_vacant("lto", self.lto.to_toml_value());
        if let Some(codegen_units) = self.codegen_units {
            set_value_if_vacant("codegen-units", codegen_units.into());
        }
        set_value_if_vacant("overflow-checks", self.overflow_checks.into());
        set_value_if_vacant("panic", self.panic.to_toml_value());
    }
}

/// The [`opt-level`](https://doc.rust-lang.org/cargo/reference/profiles.html#opt-level) setting
#[allow(unused)]
#[derive(Clone, Copy)]
pub enum OptLevel {
    O1,
    O2,
    O3,
    S,
    Z,
}

impl OptLevel {
    fn to_toml_value(&self) -> value::Value {
        match self {
            OptLevel::O1 => 1.into(),
            OptLevel::O2 => 2.into(),
            OptLevel::O3 => 3.into(),
            OptLevel::S => "s".into(),
            OptLevel::Z => "z".into(),
        }
    }
}

/// The [`link-time-optimization`](https://doc.rust-lang.org/cargo/reference/profiles.html#lto) setting.
#[derive(Clone, Copy)]
#[allow(unused)]
pub enum Lto {
    /// Sets `lto = false`
    ThinLocal,
    /// Sets `lto = "fat"`, the equivalent of `lto = true`
    Fat,
    /// Sets `lto = "thin"`
    Thin,
    /// Sets `lto = "off"`
    Off,
}

impl Lto {
    fn to_toml_value(&self) -> value::Value {
        match self {
            Lto::ThinLocal => value::Value::Boolean(false),
            Lto::Fat => value::Value::String("fat".into()),
            Lto::Thin => value::Value::String("thin".into()),
            Lto::Off => value::Value::String("off".into()),
        }
    }
}

/// The `panic` setting.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
#[allow(unused)]
pub enum PanicStrategy {
    Unwind,
    Abort,
}

impl PanicStrategy {
    fn to_toml_value(&self) -> value::Value {
        match self {
            PanicStrategy::Unwind => "unwind".into(),
            PanicStrategy::Abort => "abort".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn merge_profile_inserts_preferred_defaults() {
        let profile = Profile::default_contract_release();

        // no `[profile.release]` section specified
        let manifest_toml = "";
        let mut expected = toml::value::Table::new();
        expected.insert("opt-level".into(), value::Value::String("z".into()));
        expected.insert("lto".into(), value::Value::String("fat".into()));
        expected.insert("codegen-units".into(), value::Value::Integer(1));
        expected.insert("overflow-checks".into(), value::Value::Boolean(true));
        expected.insert("panic".into(), value::Value::String("abort".into()));

        let mut manifest_profile = toml::from_str(manifest_toml).unwrap();

        profile.merge(&mut manifest_profile);

        assert_eq!(expected, manifest_profile)
    }

    #[test]
    fn merge_profile_preserves_user_defined_settings() {
        let profile = Profile::default_contract_release();

        let manifest_toml = r#"
            panic = "unwind"
            lto = false
            opt-level = 3
            overflow-checks = false
            codegen-units = 256
        "#;
        let mut expected = toml::value::Table::new();
        expected.insert("opt-level".into(), value::Value::Integer(3));
        expected.insert("lto".into(), value::Value::Boolean(false));
        expected.insert("codegen-units".into(), value::Value::Integer(256));
        expected.insert("overflow-checks".into(), value::Value::Boolean(false));
        expected.insert("panic".into(), value::Value::String("unwind".into()));

        let mut manifest_profile = toml::from_str(manifest_toml).unwrap();

        profile.merge(&mut manifest_profile);

        assert_eq!(expected, manifest_profile)
    }
}

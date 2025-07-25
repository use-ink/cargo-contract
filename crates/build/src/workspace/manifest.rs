// Copyright (C) Use Ink (UK) Ltd.
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

use anyhow::{
    Context,
    Result,
};

use super::{
    metadata,
    Profile,
};
use crate::CrateMetadata;

use std::{
    convert::TryFrom,
    fs,
    path::{
        Path,
        PathBuf,
    },
};
use toml::value;

const MANIFEST_FILE: &str = "Cargo.toml";
const LEGACY_METADATA_PACKAGE_PATH: &str = ".ink/abi_gen";
const METADATA_PACKAGE_PATH: &str = ".ink/metadata_gen";

/// Path to a `Cargo.toml` file
#[derive(Clone, Debug)]
pub struct ManifestPath {
    path: PathBuf,
}

impl ManifestPath {
    /// Create a new [`ManifestPath`], errors if not path to `Cargo.toml`
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
    pub fn cargo_arg(&self) -> Result<String> {
        let path = self.path.canonicalize().map_err(|err| {
            anyhow::anyhow!("Failed to canonicalize {:?}: {:?}", self.path, err)
        })?;
        Ok(format!("--manifest-path={}", path.to_string_lossy()))
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

    /// Returns the absolute directory path of the manifest.
    pub fn absolute_directory(&self) -> Result<PathBuf, std::io::Error> {
        let directory = match self.directory() {
            Some(dir) => dir,
            None => Path::new("./"),
        };
        directory.canonicalize()
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
    pub fn new(manifest_path: ManifestPath) -> Result<Manifest> {
        let toml = fs::read_to_string(&manifest_path).context("Loading Cargo.toml")?;
        let toml: value::Table = toml::from_str(&toml)?;
        let mut manifest = Manifest {
            path: manifest_path,
            toml,
            metadata_package: false,
        };
        let profile = manifest.profile_release_table_mut()?;
        if profile
            .get("overflow-checks")
            .and_then(|val| val.as_bool())
            .unwrap_or(false)
        {
            anyhow::bail!("Overflow checks must be disabled. Cargo contract makes sure that no unchecked arithmetic is used.")
        }
        Ok(manifest)
    }

    /// Get the name of the package.
    fn name(&self) -> Result<&str> {
        self.toml
            .get("package")
            .ok_or_else(|| anyhow::anyhow!("package section not found"))?
            .as_table()
            .ok_or_else(|| anyhow::anyhow!("package section should be a table"))?
            .get("name")
            .ok_or_else(|| anyhow::anyhow!("package must have a name"))?
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("package name must be a string"))
    }

    /// Get a mutable reference to the `[lib]` section.
    fn lib_target_mut(&mut self) -> Result<&mut value::Table> {
        self.toml
            .get_mut("lib")
            .ok_or_else(|| anyhow::anyhow!("lib section not found"))?
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("lib section should be a table"))
    }

    /// Get mutable reference to `[lib] crate-types = []` section.
    fn crate_types_mut(&mut self) -> Result<&mut value::Array> {
        let crate_types = self
            .lib_target_mut()?
            .entry("crate-type")
            .or_insert(value::Value::Array(Default::default()));

        crate_types
            .as_array_mut()
            .ok_or_else(|| anyhow::anyhow!("crate-types should be an Array"))
    }

    /// Replaces the `[lib]` target by a single `[bin]` target using the same file and
    /// name.
    pub fn with_replaced_lib_to_bin(&mut self) -> Result<&mut Self> {
        let mut lib = self.lib_target_mut()?.clone();
        self.toml.remove("lib");
        if !lib.contains_key("name") {
            lib.insert("name".into(), self.name()?.into());
        }
        lib.remove("crate-types");
        self.toml.insert("bin".into(), vec![lib].into());
        Ok(self)
    }

    /// Add a value to the `[lib] crate-types = []` section.
    ///
    /// If the value already exists, does nothing.
    pub fn with_added_crate_type(&mut self, crate_type: &str) -> Result<&mut Self> {
        let crate_types = self.crate_types_mut()?;
        if !crate_type_exists(crate_type, crate_types) {
            crate_types.push(crate_type.into());
        }
        Ok(self)
    }

    /// Set preferred defaults for the `[profile.release]` section
    ///
    /// # Note
    ///
    /// Existing user defined settings for this section are preserved. Only if a setting
    /// is not defined is the preferred default set.
    pub fn with_profile_release_defaults(
        &mut self,
        defaults: Profile,
    ) -> Result<&mut Self> {
        let profile_release = self.profile_release_table_mut()?;
        defaults.merge(profile_release);
        Ok(self)
    }

    /// Set `[workspace]` section to an empty table. When building a contract project any
    /// workspace members are not copied to the temporary workspace, so need to be
    /// removed.
    ///
    /// Additionally, where no workspace is already specified, this can in some cases
    /// reduce the size of the contract.
    pub fn with_empty_workspace(&mut self) -> &mut Self {
        self.toml
            .insert("workspace".into(), value::Value::Table(Default::default()));
        self
    }

    /// Get mutable reference to `[profile.release]` section
    fn profile_release_table_mut(&mut self) -> Result<&mut value::Table> {
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
        let crate_types = self.crate_types_mut()?;
        if crate_type_exists(crate_type, crate_types) {
            crate_types.retain(|v| v.as_str() != Some(crate_type));
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
            eprintln!(
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

    pub fn with_dylint(&mut self) -> Result<&mut Self> {
        let ink_dylint = |lib_name: &str| {
            let mut map = value::Table::new();
            map.insert("git".into(), crate::lint::GIT_URL.into());
            //map.insert("rev".into(), crate::lint::GIT_REV.into());
            map.insert("branch".into(), "cmichi-fix-ci-2025-07-26".into());
            map.insert(
                "pattern".into(),
                value::Value::String(format!("linting/{lib_name}")),
            );
            value::Value::Table(map)
        };

        self.toml
            .entry("workspace")
            .or_insert(value::Value::Table(Default::default()))
            .as_table_mut()
            .context("workspace section should be a table")?
            .entry("metadata")
            .or_insert(value::Value::Table(Default::default()))
            .as_table_mut()
            .context("workspace.metadata section should be a table")?
            .entry("dylint")
            .or_insert(value::Value::Table(Default::default()))
            .as_table_mut()
            .context("workspace.metadata.dylint section should be a table")?
            .entry("libraries")
            .or_insert(value::Value::Array(Default::default()))
            .as_array_mut()
            .context("workspace.metadata.dylint.libraries section should be an array")?
            .extend(vec![ink_dylint("mandatory"), ink_dylint("extra")]);

        Ok(self)
    }

    /// Merge the workspace dependencies with the crate dependencies.
    pub fn with_merged_workspace_dependencies(
        &mut self,
        crate_metadata: &CrateMetadata,
    ) -> Result<&mut Self> {
        let workspace_manifest_path =
            crate_metadata.cargo_meta.workspace_root.join("Cargo.toml");

        // If the workspace manifest is the same as the crate manifest, there's not
        // workspace to fix
        if workspace_manifest_path == self.path.path {
            return Ok(self)
        }

        let workspace_toml =
            fs::read_to_string(&workspace_manifest_path).context("Loading Cargo.toml")?;
        let workspace_toml: value::Table = toml::from_str(&workspace_toml)?;

        let workspace_dependencies = workspace_toml
            .get("workspace")
            .ok_or_else(|| {
                anyhow::anyhow!("[workspace] should exist in workspace manifest")
            })?
            .as_table()
            .ok_or_else(|| anyhow::anyhow!("[workspace] should be a table"))?
            .get("dependencies");

        // If no workspace dependencies are defined, return
        let Some(workspace_dependencies) = workspace_dependencies else {
            return Ok(self)
        };

        let workspace_dependencies =
            workspace_dependencies.as_table().ok_or_else(|| {
                anyhow::anyhow!("[workspace.dependencies] should be a table")
            })?;

        merge_workspace_with_crate_dependencies(
            "dependencies",
            &mut self.toml,
            workspace_dependencies,
        )?;
        merge_workspace_with_crate_dependencies(
            "dev-dependencies",
            &mut self.toml,
            workspace_dependencies,
        )?;

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
    pub fn rewrite_relative_paths(&mut self) -> Result<()> {
        let manifest_dir = self.path.absolute_directory()?;
        let path_rewrite = PathRewrite { manifest_dir };
        path_rewrite.rewrite_relative_paths(&mut self.toml)
    }

    /// Writes the amended manifest to the given path.
    pub fn write(&self, manifest_path: &ManifestPath) -> Result<()> {
        if let Some(dir) = manifest_path.directory() {
            fs::create_dir_all(dir)
                .context(format!("Creating directory '{}'", dir.display()))?;
        }

        if self.metadata_package {
            let dir = if let Some(manifest_dir) = manifest_path.directory() {
                manifest_dir.join(METADATA_PACKAGE_PATH)
            } else {
                METADATA_PACKAGE_PATH.into()
            };

            fs::create_dir_all(&dir)
                .context(format!("Creating directory '{}'", dir.display()))?;

            let contract_package_name = self
                .toml
                .get("package")
                .ok_or_else(|| anyhow::anyhow!("package section not found"))?
                .get("name")
                .ok_or_else(|| anyhow::anyhow!("[package] name field not found"))?
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("[package] name should be a string"))?;

            let ink_crate = self
                .toml
                .get("dependencies")
                .ok_or_else(|| anyhow::anyhow!("[dependencies] section not found"))?
                .get("ink")
                .ok_or_else(|| anyhow::anyhow!("ink dependency not found"))?
                .as_table()
                .ok_or_else(|| anyhow::anyhow!("ink dependency should be a table"))?;

            let features = self
                .toml
                .get("features")
                .ok_or_else(|| anyhow::anyhow!("[features] section not found"))?
                .as_table()
                .ok_or_else(|| anyhow::anyhow!("[features] section should be a table"))?;

            metadata::generate_package(
                dir,
                contract_package_name,
                ink_crate.clone(),
                features,
            )?;
        }

        let updated_toml = toml::to_string(&self.toml)?;
        tracing::debug!(
            "Writing updated manifest to '{}'",
            manifest_path.as_ref().display()
        );
        fs::write(manifest_path, updated_toml)?;
        Ok(())
    }
}

/// Replace relative paths with absolute paths with the working directory.
struct PathRewrite {
    manifest_dir: PathBuf,
}

impl PathRewrite {
    /// Replace relative paths with absolute paths with the working directory.
    fn rewrite_relative_paths(&self, toml: &mut value::Table) -> Result<()> {
        // Rewrite `[package.build]` path to an absolute path.
        if let Some(package) = toml.get_mut("package") {
            let package = package
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("`[package]` should be a table"))?;
            if let Some(build) = package.get_mut("build") {
                self.to_absolute_path("[package.build]".to_string(), build)?
            }
        }

        // Rewrite `[lib] path = /path/to/lib.rs`
        if let Some(lib) = toml.get_mut("lib") {
            self.rewrite_path(lib, "lib", "src/lib.rs")?;
        }

        // Rewrite `[[bin]] path = /path/to/main.rs`
        if let Some(bin) = toml.get_mut("bin") {
            let bins = bin.as_array_mut().ok_or_else(|| {
                anyhow::anyhow!("'[[bin]]' section should be a table array")
            })?;

            // Rewrite `[[bin]] path =` value to an absolute path.
            for bin in bins {
                self.rewrite_path(bin, "[bin]", "src/main.rs")?;
            }
        }

        self.rewrite_dependencies_relative_paths(toml, "dependencies")?;
        self.rewrite_dependencies_relative_paths(toml, "dev-dependencies")?;

        Ok(())
    }

    fn rewrite_path(
        &self,
        table_value: &mut value::Value,
        table_section: &str,
        default: &str,
    ) -> Result<()> {
        let table = table_value.as_table_mut().ok_or_else(|| {
            anyhow::anyhow!("'[{}]' section should be a table", table_section)
        })?;

        match table.get_mut("path") {
            Some(existing_path) => {
                self.to_absolute_path(format!("[{table_section}]/path"), existing_path)
            }
            None => {
                let default_path = PathBuf::from(default);
                if !default_path.exists() {
                    anyhow::bail!(
                        "No path specified, and the default `{}` was not found",
                        default
                    )
                }
                let path = self.manifest_dir.join(default_path);
                tracing::debug!("Adding default path '{}'", path.display());
                table.insert(
                    "path".into(),
                    value::Value::String(path.to_string_lossy().into()),
                );
                Ok(())
            }
        }
    }

    /// Expand a relative path to an absolute path.
    fn to_absolute_path(
        &self,
        value_id: String,
        existing_path: &mut value::Value,
    ) -> Result<()> {
        let path_str = existing_path
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{} should be a string", value_id))?;
        #[cfg(windows)]
        // On Windows path separators are `\`, hence we need to replace the `/` in
        // e.g. `src/lib.rs`.
        let path_str = &path_str.replace("/", "\\");
        let path = PathBuf::from(path_str);
        if path.is_relative() {
            let lib_abs = self.manifest_dir.join(path);
            tracing::debug!("Rewriting {} to '{}'", value_id, lib_abs.display());
            *existing_path = value::Value::String(lib_abs.to_string_lossy().into())
        }
        Ok(())
    }

    /// Rewrite the relative paths of dependencies.
    fn rewrite_dependencies_relative_paths(
        &self,
        toml: &mut value::Table,
        section_name: &str,
    ) -> Result<()> {
        if let Some(dependencies) = toml.get_mut(section_name) {
            let table = dependencies
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("dependencies should be a table"))?;
            for (name, value) in table {
                let package_name = {
                    let package = value.get("package");
                    let package_name = package.and_then(|p| p.as_str()).unwrap_or(name);
                    package_name.to_string()
                };

                if let Some(dependency) = value.as_table_mut() {
                    if let Some(dep_path) = dependency.get_mut("path") {
                        self.to_absolute_path(
                            format!("dependency {package_name}"),
                            dep_path,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn crate_type_exists(crate_type: &str, crate_types: &[value::Value]) -> bool {
    crate_types.iter().any(|v| v.as_str() == Some(crate_type))
}

fn merge_workspace_with_crate_dependencies(
    section_name: &str,
    crate_toml: &mut value::Table,
    workspace_dependencies: &value::Table,
) -> Result<()> {
    let Some(dependencies) = crate_toml.get_mut(section_name) else {
        return Ok(())
    };

    let table = dependencies
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("dependencies should be a table"))?;

    for (name, value) in table {
        let Some(dependency) = value.as_table_mut() else {
            continue
        };

        let is_workspace_dependency = dependency
            .get_mut("workspace")
            .unwrap_or(&mut toml::Value::Boolean(false))
            .as_bool()
            .unwrap_or(false);
        if !is_workspace_dependency {
            continue
        }

        let workspace_dependency = workspace_dependencies.get(name).ok_or_else(|| {
            anyhow::anyhow!("'{}' is not a key in workspace_dependencies", name)
        })?;
        let workspace_dependency = match workspace_dependency {
            toml::Value::Table(table) => table.to_owned(),
            // If the workspace dependency is just a version string, we create a table
            toml::Value::String(version) => {
                let mut table = toml::value::Table::new();
                table.insert("version".to_string(), toml::Value::String(version.clone()));
                table
            }
            // If the workspace dependency is invalid, we throw an error
            _ => {
                anyhow::bail!("Invalid workspace dependency for {}", name);
            }
        };

        dependency.remove("workspace");
        for (key, value) in workspace_dependency {
            if let Some(config) = dependency.get_mut(&key) {
                // If it's an array we merge the values,
                // otherwise we keep the crate value.
                if let toml::Value::Array(value) = value {
                    if let toml::Value::Array(config) = config {
                        config.extend(value.clone());

                        let mut new_config = Vec::new();
                        for v in config.iter() {
                            if !new_config.contains(v) {
                                new_config.push(v.clone());
                            }
                        }
                        *config = new_config;
                    }
                }
            } else {
                dependency.insert(key.clone(), value.clone());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::ManifestPath;
    use crate::util::tests::with_tmp_dir;
    use std::fs;

    #[test]
    fn must_return_absolute_path_from_absolute_path() {
        with_tmp_dir(|path| {
            // given
            let cargo_toml_path = path.join("Cargo.toml");
            let _ = fs::File::create(&cargo_toml_path).expect("file creation failed");
            let manifest_path = ManifestPath::new(cargo_toml_path)
                .expect("manifest path creation failed");

            // when
            let absolute_path = manifest_path
                .absolute_directory()
                .expect("absolute path extraction failed");

            // then
            assert_eq!(absolute_path.as_path(), path);
            Ok(())
        })
    }
}

// Copyright 2018-2022 Parity Technologies (UK) Ltd.
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

use crate::ManifestPath;
use anyhow::{
    Context,
    Result,
};
use std::{
    fs,
    ops::Deref,
    path::{
        Path,
        PathBuf,
    },
};
use toml::value;

/// Creates a temporary directory and passes the `tmp_dir` path to `f`.
/// Panics if `f` returns an `Err`.
pub fn with_tmp_dir<F>(f: F)
where
    F: FnOnce(&Path) -> Result<()>,
{
    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.test.")
        .tempdir()
        .expect("temporary directory creation failed");

    // catch test panics in order to clean up temp dir which will be very large
    f(&tmp_dir.path().canonicalize().unwrap()).expect("Error executing test with tmp dir")
}

/// Creates a new contract into a temporary directory. The contract's
/// `ManifestPath` is passed into `f`.
pub fn with_new_contract_project<F>(f: F)
where
    F: FnOnce(ManifestPath) -> Result<()>,
{
    with_tmp_dir(|tmp_dir| {
        let project_name = "new_project";
        crate::new_contract_project(project_name, Some(tmp_dir))
            .expect("new project creation failed");
        let working_dir = tmp_dir.join(project_name);
        let manifest_path = ManifestPath::new(working_dir.join("Cargo.toml"))?;

        f(manifest_path)
    })
}

/// Deletes the mocked executable on `Drop`.
pub struct MockGuard(PathBuf);

impl Drop for MockGuard {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
    }
}

impl Deref for MockGuard {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Creates an executable file at `path` with the content `content`.
///
/// Currently works only on `unix`.
#[cfg(unix)]
pub fn create_executable(path: &Path, content: &str) -> MockGuard {
    use std::{
        env,
        io::Write,
        os::unix::fs::PermissionsExt,
    };
    let mut guard = MockGuard(path.to_path_buf());
    let mut file = std::fs::File::create(path).unwrap();
    let path = path.canonicalize().unwrap();
    guard.0 = path.clone();
    file.write_all(content.as_bytes())
        .expect("writing of executable failed");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o777))
        .expect("setting permissions failed");

    // make sure the mocked executable is in the path
    let env_paths = {
        let work_dir = path.parent().unwrap().to_path_buf();
        let pathes = env::var_os("PATH").unwrap_or_default();
        let mut pathes: Vec<_> = env::split_paths(&pathes).collect();
        if !pathes.contains(&work_dir) {
            pathes.insert(0, work_dir);
        }
        pathes
    };
    env::set_var("PATH", env::join_paths(env_paths).unwrap());
    guard
}

/// Modify a contracts `Cargo.toml` for testing purposes
pub struct TestContractManifest {
    toml: value::Table,
    manifest_path: ManifestPath,
}

impl TestContractManifest {
    pub fn new(manifest_path: ManifestPath) -> Result<Self> {
        Ok(Self {
            toml: serde_json::from_slice(&fs::read(&manifest_path)?)?,
            manifest_path,
        })
    }

    fn package_mut(&mut self) -> Result<&mut value::Table> {
        self.toml
            .get_mut("package")
            .context("package section not found")?
            .as_table_mut()
            .context("package section should be a table")
    }

    /// Add a key/value to the `[package.metadata.contract.user]` section
    pub fn add_user_metadata_value(
        &mut self,
        key: &'static str,
        value: value::Value,
    ) -> Result<()> {
        self.package_mut()?
            .entry("metadata")
            .or_insert(value::Value::Table(Default::default()))
            .as_table_mut()
            .context("metadata section should be a table")?
            .entry("contract")
            .or_insert(value::Value::Table(Default::default()))
            .as_table_mut()
            .context("metadata.contract section should be a table")?
            .entry("user")
            .or_insert(value::Value::Table(Default::default()))
            .as_table_mut()
            .context("metadata.contract.user section should be a table")?
            .insert(key.into(), value);
        Ok(())
    }

    pub fn add_package_value(
        &mut self,
        key: &'static str,
        value: value::Value,
    ) -> Result<()> {
        self.package_mut()?.insert(key.into(), value);
        Ok(())
    }

    /// Set `optimization-passes` in `[package.metadata.contract]`
    pub fn set_profile_optimization_passes<P>(
        &mut self,
        passes: P,
    ) -> Result<Option<value::Value>>
    where
        P: ToString,
    {
        Ok(self
            .toml
            .entry("package")
            .or_insert(value::Value::Table(Default::default()))
            .as_table_mut()
            .context("package section should be a table")?
            .entry("metadata")
            .or_insert(value::Value::Table(Default::default()))
            .as_table_mut()
            .context("metadata section should be a table")?
            .entry("contract")
            .or_insert(value::Value::Table(Default::default()))
            .as_table_mut()
            .context("metadata.contract section should be a table")?
            .insert(
                "optimization-passes".to_string(),
                value::Value::String(passes.to_string()),
            ))
    }

    /// Set the dependency version of `package` to `version`.
    pub fn set_dependency_version(
        &mut self,
        dependency: &str,
        version: &str,
    ) -> Result<Option<toml::Value>> {
        Ok(self
            .toml
            .get_mut("dependencies")
            .ok_or_else(|| anyhow::anyhow!("[dependencies] section not found"))?
            .get_mut(dependency)
            .ok_or_else(|| anyhow::anyhow!("{} dependency not found", dependency))?
            .as_table_mut()
            .ok_or_else(|| {
                anyhow::anyhow!("{} dependency should be a table", dependency)
            })?
            .insert("version".into(), value::Value::String(version.into())))
    }

    /// Set the `lib` name to `name`.
    pub fn set_lib_name(&mut self, name: &str) -> Result<Option<toml::Value>> {
        Ok(self
            .toml
            .get_mut("lib")
            .ok_or_else(|| anyhow::anyhow!("[lib] section not found"))?
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("[lib] should be a table"))?
            .insert("name".into(), value::Value::String(name.into())))
    }

    /// Set the `package` name to `name`.
    pub fn set_package_name(&mut self, name: &str) -> Result<Option<toml::Value>> {
        Ok(self
            .toml
            .get_mut("package")
            .ok_or_else(|| anyhow::anyhow!("[package] section not found"))?
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("[package] should be a table"))?
            .insert("name".into(), value::Value::String(name.into())))
    }

    /// Set the `lib` path to `path`.
    pub fn set_lib_path(&mut self, path: &str) -> Result<Option<toml::Value>> {
        Ok(self
            .toml
            .get_mut("lib")
            .ok_or_else(|| anyhow::anyhow!("[lib] section not found"))?
            .as_table_mut()
            .ok_or_else(|| anyhow::anyhow!("[lib] should be a table"))?
            .insert("path".into(), value::Value::String(path.into())))
    }

    pub fn write(&self) -> Result<()> {
        let toml = toml::to_string(&self.toml)?;
        fs::write(&self.manifest_path, toml).map_err(Into::into)
    }
}

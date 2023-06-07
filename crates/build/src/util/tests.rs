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
    fs::File,
    io::Write,
    ops::Deref,
    path::{
        Path,
        PathBuf,
    },
    sync::atomic::{
        AtomicU32,
        Ordering,
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

/// Global counter to generate unique contract names in `with_new_contract_project`.
///
/// We typically use `with_tmp_dir` to generate temporary folders to build contracts
/// in. But for caching purposes our CI uses `CARGO_TARGET_DIR` to overwrite the
/// target directory of any contract build -- it is set to a fixed cache directory
/// instead.
/// This poses a problem since we still want to ensure that each test builds to its
/// own, unique target directory -- without interfering with the target directory of
/// other tests. In the past this has been a problem when a test tried to create a
/// contract with the same contract name as another test -- both were then build
/// into the same target directory, sometimes causing test failures for strange reasons.
///
/// The fix we decided on is to append a unique number to each contract name which
/// is created. This `COUNTER` provides a global counter which is accessed by each test
/// (in each thread) to get the current `COUNTER` number and increase it afterwards.
///
/// We decided to go for this counter instead of hashing (with e.g. the temp dir) to
/// prevent an infinite number of contract artifacts being created in the cache directory.
static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Creates a new contract into a temporary directory. The contract's
/// `ManifestPath` is passed into `f`.
pub fn with_new_contract_project_unique<F>(f: F)
where
    F: FnOnce(ManifestPath) -> anyhow::Result<()>,
{
    with_tmp_dir(|tmp_dir| {
        let unique_name =
            format!("new_project_{}", COUNTER.fetch_add(1, Ordering::SeqCst));

        crate::cmd::new::execute(&unique_name, Some(tmp_dir))
            .expect("new project creation failed");
        let working_dir = tmp_dir.join(unique_name);
        let manifest_path = ManifestPath::new(working_dir.join("Cargo.toml"))?;

        f(manifest_path)
    })
}

/// Creates many new subcontracts into a temporary directory.
pub fn with_new_subcontract_projects<F>(f: F, n: u32)
where
    F: FnOnce(PathBuf) -> anyhow::Result<()>,
{
    with_tmp_dir(|tmp_dir| {
        let mut unique_names = Vec::new();
        for _ in 0..n {
            let unique_name =
                format!("new_project_{}", COUNTER.fetch_add(1, Ordering::SeqCst));
            crate::cmd::new::execute(&unique_name, Some(tmp_dir))
                .expect("new project creation failed");
            unique_names.push(unique_name);
        }

        let manifest_path = tmp_dir.join("Cargo.toml");

        let mut output = File::create(manifest_path.clone())?;
        write!(output, "[workspace]\n\n")?;
        writeln!(output, "members = [")?;
        for unique_name in unique_names {
            writeln!(output, "  \"{}\",", unique_name)?;
        }
        write!(output, "]")?;

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
            toml: toml::from_str(&fs::read_to_string(&manifest_path)?)?,
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

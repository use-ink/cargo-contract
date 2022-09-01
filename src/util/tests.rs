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
use anyhow::Result;
use std::{
    fs,
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
pub fn with_new_contract_project<F>(f: F)
where
    F: FnOnce(ManifestPath) -> Result<()>,
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
#[cfg(all(unix, feature = "test-ci-only"))]
pub fn create_executable(path: &Path, content: &str) -> MockGuard {
    use std::{
        env,
        io::Write,
        os::unix::fs::PermissionsExt,
    };
    let mut guard = MockGuard(path.to_path_buf());
    let mut file = std::fs::File::create(&path).unwrap();
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
    env::set_var("PATH", env::join_paths(&env_paths).unwrap());
    guard
}

/// Init a tracing subscriber for logging in tests.
///
/// Be aware that this enables `TRACE` by default. It also ignores any error
/// while setting up the logger.
///
/// The logs are not shown by default, logs are only shown when the test fails
/// or if [`nocapture`](https://doc.rust-lang.org/cargo/commands/cargo-test.html#display-options)
/// is being used.
#[cfg(any(feature = "integration-tests", feature = "test-ci-only"))]
pub fn init_tracing_subscriber() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_test_writer()
        .try_init();
}

/// Enables running a group of tests sequentially, each starting with the original template
/// contract, but maintaining the target directory so compilation artifacts are maintained across
/// each test.
pub struct BuildTestContext {
    template_dir: PathBuf,
    working_dir: PathBuf,
}

impl BuildTestContext {
    /// Create a new `BuildTestContext`, running the `new` command to create a blank contract
    /// template project for testing the build process.
    pub fn new(tmp_dir: &Path, working_project_name: &str) -> Result<Self> {
        crate::cmd::new::execute(working_project_name, Some(tmp_dir))
            .expect("new project creation failed");
        let working_dir = tmp_dir.join(working_project_name);

        let template_dir = tmp_dir.join(format!("{}_template", working_project_name));

        fs::rename(&working_dir, &template_dir)?;
        copy_dir_all(&template_dir, &working_dir)?;

        Ok(Self {
            template_dir,
            working_dir,
        })
    }

    /// Run the supplied test. Test failure will print the error to `stdout`, and this will still
    /// return `Ok(())` in order that subsequent tests will still be run.
    ///
    /// The test may modify the contracts project files (e.g. Cargo.toml, lib.rs), so after
    /// completion those files are reverted to their original state for the next test.
    ///
    /// Importantly, the `target` directory is maintained so as to avoid recompiling all of the
    /// dependencies for each test.
    pub fn run_test(
        &self,
        name: &str,
        test: impl FnOnce(&ManifestPath) -> Result<()>,
    ) -> Result<()> {
        println!("Running {}", name);
        let manifest_path = ManifestPath::new(self.working_dir.join("Cargo.toml"))?;
        match test(&manifest_path) {
            Ok(()) => (),
            Err(err) => {
                println!("{} FAILED: {:?}", name, err);
            }
        }
        // revert to the original template files, but keep the `target` dir from the previous run.
        self.remove_all_except_target_dir()?;
        copy_dir_all(&self.template_dir, &self.working_dir)?;
        Ok(())
    }

    /// Deletes all files and folders in project dir (except the `target` directory)
    fn remove_all_except_target_dir(&self) -> Result<()> {
        for entry in fs::read_dir(&self.working_dir)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_dir() {
                // remove all except the target dir
                if !entry.file_name() == "target" {
                    fs::remove_dir_all(entry.path())?
                }
            } else {
                fs::remove_file(entry.path())?
            }
        }
        Ok(())
    }
}

/// Copy contents of `src` to `dst` recursively.
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

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

use crate::Verbosity;
use anyhow::{
    Context,
    Result,
};
use heck::ToUpperCamelCase as _;
use rustc_version::Channel;
use semver::Version;
use std::{
    ffi::OsStr,
    fs,
    io::{
        Cursor,
        Read,
        Seek,
        SeekFrom,
        Write,
    },
    path::{
        Path,
        PathBuf,
    },
    process::Command,
};

/// This makes sure we are building with a minimum `stable` toolchain version.
fn assert_channel() -> Result<()> {
    let meta = rustc_version::version_meta()?;
    let min_version = Version::new(1, 63, 0);
    match meta.channel {
        Channel::Stable if meta.semver >= min_version => Ok(()),
        Channel::Stable => {
            anyhow::bail!(
                "The minimum Rust version is {}. You are using {}.",
                min_version,
                meta.semver
            )
        }
        _ => {
            anyhow::bail!(
                "Using the {:?} channel is not supported. \
                Contracts should be built using a \"stable\" toolchain.",
                format!("{:?}", meta.channel).to_lowercase(),
            )
        }
    }
}

/// Invokes `cargo` with the subcommand `command` and the supplied `args`.
///
/// In case `working_dir` is set, the command will be invoked with that folder
/// as the working directory.
///
/// In case `env` is given environment variables can be either set or unset:
///   * To _set_ push an item a la `("VAR_NAME", Some("VAR_VALUE"))` to
///     the `env` vector.
///   * To _unset_ push an item a la `("VAR_NAME", None)` to the `env`
///     vector.
///
/// If successful, returns the stdout bytes.
pub(crate) fn invoke_cargo<I, S, P>(
    command: &str,
    args: I,
    working_dir: Option<P>,
    verbosity: Verbosity,
    env: Vec<(&str, Option<&str>)>,
) -> Result<Vec<u8>>
where
    I: IntoIterator<Item = S> + std::fmt::Debug,
    S: AsRef<OsStr>,
    P: AsRef<Path>,
{
    assert_channel()?;
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = Command::new(cargo);

    env.iter().for_each(|(env_key, maybe_env_val)| {
        match maybe_env_val {
            Some(env_val) => cmd.env(env_key, env_val),
            None => cmd.env_remove(env_key),
        };
    });

    if let Some(path) = working_dir {
        tracing::debug!("Setting cargo working dir to '{}'", path.as_ref().display());
        cmd.current_dir(path);
    }

    // Allow nightly features on a stable toolchain
    cmd.env("RUSTC_BOOTSTRAP", "1");

    cmd.arg(command);
    cmd.args(args);
    match verbosity {
        Verbosity::Quiet => cmd.arg("--quiet"),
        Verbosity::Verbose => {
            if command != "dylint" {
                cmd.arg("--verbose")
            } else {
                &mut cmd
            }
        }
        Verbosity::Default => &mut cmd,
    };

    tracing::debug!("Invoking cargo: {:?}", cmd);

    let child = cmd
        // capture the stdout to return from this function as bytes
        .stdout(std::process::Stdio::piped())
        .spawn()
        .context(format!("Error executing `{:?}`", cmd))?;
    let output = child.wait_with_output()?;

    if output.status.success() {
        Ok(output.stdout)
    } else {
        anyhow::bail!(
            "`{:?}` failed with exit code: {:?}",
            cmd,
            output.status.code()
        );
    }
}

/// Returns the base name of the path.
pub(crate) fn base_name(path: &Path) -> &str {
    path.file_name()
        .expect("file name must exist")
        .to_str()
        .expect("must be valid utf-8")
}

/// Decode hex string with or without 0x prefix
pub fn decode_hex(input: &str) -> Result<Vec<u8>, hex::FromHexError> {
    hex::decode(input.trim_start_matches("0x"))
}

/// Prints to stdout if `verbosity.is_verbose()` is `true`.
#[macro_export]
macro_rules! maybe_println {
    ($verbosity:expr, $($msg:tt)*) => {
        if $verbosity.is_verbose() {
            ::std::println!($($msg)*);
        }
    };
}

pub const DEFAULT_KEY_COL_WIDTH: usize = 12;

/// Pretty print name value, name right aligned with colour.
#[macro_export]
macro_rules! name_value_println {
    ($name:tt, $value:expr, $width:expr) => {{
        use colored::Colorize as _;
        ::std::println!(
            "{:>width$} {}",
            $name.bright_purple().bold(),
            $value.bright_white(),
            width = $width,
        );
    }};
    ($name:tt, $value:expr) => {
        $crate::name_value_println!($name, $value, $crate::DEFAULT_KEY_COL_WIDTH)
    };
}

#[cfg(test)]
pub mod tests {
    use crate::ManifestPath;
    use std::{
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
        F: FnOnce(&Path) -> anyhow::Result<()>,
    {
        let tmp_dir = tempfile::Builder::new()
            .prefix("cargo-contract.test.")
            .tempdir()
            .expect("temporary directory creation failed");

        // catch test panics in order to clean up temp dir which will be very large
        f(&tmp_dir.path().canonicalize().unwrap())
            .expect("Error executing test with tmp dir")
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
}

// Unzips the file at `template` to `out_dir`.
//
// In case `name` is set the zip file is treated as if it were a template for a new
// contract. Replacements in `Cargo.toml` for `name`-placeholders are attempted in
// that case.
pub fn unzip(template: &[u8], out_dir: PathBuf, name: Option<&str>) -> Result<()> {
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_all(template)?;
    cursor.seek(SeekFrom::Start(0))?;

    let mut archive = zip::ZipArchive::new(cursor)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = out_dir.join(file.name());

        if (*file.name()).ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(&p)?;
                }
            }
            let mut outfile = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(outpath.clone())
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::AlreadyExists {
                        anyhow::anyhow!("File {} already exists", file.name(),)
                    } else {
                        anyhow::anyhow!(e)
                    }
                })?;

            if let Some(name) = name {
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                let contents = contents.replace("{{name}}", name);
                let contents =
                    contents.replace("{{camel_name}}", &name.to_upper_camel_case());
                outfile.write_all(contents.as_bytes())?;
            } else {
                let mut v = Vec::new();
                file.read_to_end(&mut v)?;
                outfile.write_all(v.as_slice())?;
            }
        }

        // Get and set permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

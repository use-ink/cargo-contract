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

#[cfg(test)]
pub mod tests;

use crate::Verbosity;
use anyhow::{
    Context,
    Result,
};
use cargo_metadata::PackageId;
use heck::ToUpperCamelCase as _;
use regex::Regex;
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

/// PackageId looks like this:
/// `subcontract 3.0.0 (path+file:///path/to/subcontract)`
/// so we have to extract the package name via regex:
pub fn extract_subcontract_name(package_id: PackageId) -> Option<String> {
    let re = Regex::new(r"([^\s]+)").ok()?;
    let caps = re.captures(package_id.repr.as_str())?;
    let package = caps.get(1)?.as_str();
    Some(String::from(package))
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
            $value,
            width = $width,
        );
    }};
    ($name:tt, $value:expr) => {
        $crate::name_value_println!($name, $value, $crate::DEFAULT_KEY_COL_WIDTH)
    };
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

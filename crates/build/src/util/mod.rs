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
use anyhow::Result;
use duct::Expression;
use std::{
    ffi::OsString,
    path::Path,
};
use term_size as _;

// Returns the current Rust toolchain formatted by `<channel>-<target-triple>`.
pub(crate) fn rust_toolchain() -> Result<String> {
    let meta = rustc_version::version_meta()?;
    let toolchain = format!("{:?}-{}", meta.channel, meta.host,).to_lowercase();

    Ok(toolchain)
}

/// Builds an [`Expression`] for invoking `cargo`.
///
/// In case `working_dir` is set, the command will be invoked with that folder
/// as the working directory.
///
/// In case `env` is given environment variables can be either set or unset:
///   * To _set_ push an item a la `("VAR_NAME", Some("VAR_VALUE"))` to the `env` vector.
///   * To _unset_ push an item a la `("VAR_NAME", None)` to the `env` vector.
pub fn cargo_cmd<I, S, P>(
    command: &str,
    args: I,
    working_dir: Option<P>,
    verbosity: Verbosity,
    env: Vec<(&str, Option<String>)>,
) -> Expression
where
    I: IntoIterator<Item = S> + std::fmt::Debug,
    S: Into<OsString>,
    P: AsRef<Path>,
{
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd_args = Vec::new();

    cmd_args.push(command);
    if command != "dylint" {
        cmd_args.push("--color=always");
    }

    match verbosity {
        Verbosity::Quiet => cmd_args.push("--quiet"),
        Verbosity::Verbose => {
            if command != "dylint" {
                cmd_args.push("--verbose")
            }
        }
        Verbosity::Default => (),
    };

    let mut cmd_args: Vec<OsString> = cmd_args.iter().map(Into::into).collect();
    for arg in args {
        cmd_args.push(arg.into());
    }

    let mut cmd = duct::cmd(cargo, &cmd_args);

    env.iter().for_each(|(env_key, maybe_env_val)| {
        match maybe_env_val {
            Some(env_val) => cmd = cmd.env(env_key, env_val),
            None => cmd = cmd.env_remove(env_key),
        };
    });

    if let Some(path) = working_dir {
        tracing::debug!("Setting cargo working dir to '{}'", path.as_ref().display());
        cmd = cmd.dir(path.as_ref());
    }

    cmd
}

/// Configures the cargo command to output colour and the progress bar.
pub fn cargo_tty_output(cmd: Expression) -> Expression {
    #[cfg(windows)]
    let term_size = "100";

    #[cfg(not(windows))]
    let term_size = term_size::dimensions_stderr()
        .map(|(width, _)| width.to_string())
        .unwrap_or_else(|| "100".to_string());

    cmd.env("CARGO_TERM_COLOR", "auto")
        .env("CARGO_TERM_PROGRESS_WIDTH", term_size)
        .env("CARGO_TERM_PROGRESS_WHEN", "auto")
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

/// Prints to stderr if `verbosity.is_verbose()` is `true`.
/// Like `cargo`, we use stderr for verbose output.
#[macro_export]
macro_rules! maybe_println {
    ($verbosity:expr, $($msg:tt)*) => {
        if $verbosity.is_verbose() {
            ::std::eprintln!($($msg)*);
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

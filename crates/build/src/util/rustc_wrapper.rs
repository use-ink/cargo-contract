// Copyright (C) ink! contributors.
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

//! Utilities for generating and setting a `rustc` wrapper executable for `cargo`
//! commands.
//!
//! # Motivation
//!
//! The custom `rustc` wrapper passes extra compiler flags to `rustc`.
//! This is useful in cases where `cargo` won't pass compiler flags to `rustc`
//! for some compiler invocations
//! (e.g. `cargo` doesn't pass `rustc` flags to proc macros and build scripts
//! when the `--target` flag is set).
//!
//! Ref: <https://doc.rust-lang.org/cargo/reference/config.html#buildrustflags>
//!
//! Ref: <https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-reads>

use std::{
    env,
    fs,
    path::Path,
};

use anyhow::{
    Context,
    Result,
};

use crate::{
    util,
    util::EnvVars,
    CrateMetadata,
    Verbosity,
};

/// Generates a `rustc` wrapper executable and returns its path.
///
/// See [`crate::rustc_wrapper`] module docs for motivation.
pub fn generate<P: AsRef<Path>>(target_dir: P) -> Result<String> {
    let dir = target_dir.as_ref().join("rustc_wrapper");
    fs::create_dir_all(&dir)?;
    tracing::debug!("Generating `rustc` wrapper executable in {}", dir.display());

    // Creates `rustc` wrapper project.
    let cargo_toml = include_str!("../../templates/rustc_wrapper/_Cargo.toml");
    let main_rs = include_str!("../../templates/rustc_wrapper/main.rs");
    let manifest_path = dir.join("Cargo.toml");
    fs::write(&manifest_path, cargo_toml)?;
    fs::write(dir.join("main.rs"), main_rs)?;

    // Compiles `rustc` wrapper.
    let args = [
        format!("--manifest-path={}", manifest_path.display()),
        "--release".to_string(),
        // JSON output is easier to parse.
        "--message-format=json".to_string(),
    ];
    let cmd = util::cargo_cmd("build", args, Some(&dir), Verbosity::Quiet, Vec::new());
    let output = cmd.stdout_capture().stderr_capture().run()?;
    if !output.status.success() {
        let error_msg = "Failed to generate `rustc` wrapper";
        if output.stderr.is_empty() {
            anyhow::bail!(error_msg)
        } else {
            anyhow::bail!("{}: {}", error_msg, String::from_utf8_lossy(&output.stderr))
        }
    }

    // Parses JSON output for path to executable.
    // Ref: <https://doc.rust-lang.org/cargo/reference/external-tools.html#artifact-messages>
    // Ref: <https://doc.rust-lang.org/rustc/json.html>
    let stdout = String::from_utf8_lossy(&output.stdout);
    let exec_path_str = stdout.lines().find_map(|line| {
        if !line.contains("\"compiler-artifact\"") {
            return None;
        }
        let json: serde_json::Value = serde_json::from_str(line).ok()?;
        let reason = json.get("reason")?;
        if reason != "compiler-artifact" {
            return None;
        }
        let exec = json.get("executable")?;
        exec.as_str().map(ToString::to_string)
    });
    exec_path_str.context("Failed to generate `rustc` wrapper")
}

/// Returns a list env vars required to set a custom `rustc` wrapper and ABI `cfg` flags
/// (if necessary).
///
/// # Note
///
/// See [`crate::rustc_wrapper`] module docs for motivation.
///
/// The `rustc` wrapper is set via cargo's `RUSTC_WRAPPER` env var.
///
/// The extra compiler flags to pass are specified via the `RUSTC_WRAPPER_ENCODED_FLAGS`
/// env var.
pub fn env_vars(crate_metadata: &CrateMetadata) -> Result<Option<EnvVars<'_>>> {
    if let Some(abi) = crate_metadata.abi {
        let rustc_wrapper = env::var("INK_RUSTC_WRAPPER")
            .context("Failed to retrieve `rustc` wrapper from environment")
            .or_else(|_| generate(&crate_metadata.target_directory))?;
        if env::var("INK_RUSTC_WRAPPER").is_err() {
            // SAFETY: The `rustc` wrapper is safe to reuse across all threads.
            env::set_var("INK_RUSTC_WRAPPER", &rustc_wrapper);
        }
        return Ok(Some(vec![
            ("RUSTC_WRAPPER", Some(rustc_wrapper)),
            (
                "RUSTC_WRAPPER_ENCODED_FLAGS",
                Some(abi.cargo_encoded_rustflag()),
            ),
        ]))
    }

    Ok(None)
}

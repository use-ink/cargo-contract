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

use crate::{
    OptimizationPasses,
    OptimizationResult,
};

use anyhow::Result;
use colored::Colorize;
use regex::Regex;

use std::{
    fs::metadata,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
    str,
};

const WASM_OPT_INSTALLATION_SUGGESTION: &str =
    "wasm-opt not found! Make sure the binary is in your PATH environment.\n\n\
    We use this tool to optimize the size of your contract's Wasm binary.\n\n\
    wasm-opt is part of the binaryen package. You can find detailed\n\
    installation instructions on https://github.com/WebAssembly/binaryen#tools.\n\n\
    There are ready-to-install packages for many platforms:\n\
    * Debian/Ubuntu: apt-get install binaryen\n\
    * Homebrew: brew install binaryen\n\
    * Arch Linux: pacman -S binaryen\n\
    * Windows: binary releases at https://github.com/WebAssembly/binaryen/releases";

/// A helpful struct for interacting with Binaryen's `wasm-opt` tool.
pub struct WasmOptHandler {
    /// The path to the `wasm-opt` binary.
    wasm_opt_path: PathBuf,
    /// The optimization level that should be used when optimizing the Wasm binary.
    optimization_level: OptimizationPasses,
    /// Whether or not to keep debugging information in the final Wasm binary.
    keep_debug_symbols: bool,
    /// The version number of the `wasm-opt` binary being executed.
    version: u32,
}

impl WasmOptHandler {
    /// Generate a new instance of the handler.
    ///
    /// Fails if the `wasm-opt` binary is not installed on the system, or if an outdated `wasm-opt`
    /// binary is used (currently a version >= 99 is required).
    pub fn new(
        optimization_level: OptimizationPasses,
        keep_debug_symbols: bool,
    ) -> Result<Self> {
        let which = which::which("wasm-opt");
        if which.is_err() {
            anyhow::bail!(WASM_OPT_INSTALLATION_SUGGESTION.to_string().bright_yellow());
        }

        let wasm_opt_path =
            which.expect("we just checked if `which` returned an err; qed");
        tracing::debug!("Path to wasm-opt executable: {}", wasm_opt_path.display());

        let version =
            Self::check_wasm_opt_version_compatibility(wasm_opt_path.as_path())?;

        Ok(Self {
            wasm_opt_path,
            optimization_level,
            keep_debug_symbols,
            version,
        })
    }

    /// Attempts to perform optional Wasm optimization using Binaryen's `wasm-opt` tool.
    ///
    /// If successful, the optimized Wasm binary is written to `dest_wasm`.
    pub fn optimize(
        &self,
        dest_wasm: &PathBuf,
        contract_artifact_name: &String,
    ) -> Result<OptimizationResult> {
        // We'll create a temporary file for our optimized Wasm binary. Note that we'll later
        // overwrite this with the original path of the Wasm binary.
        let mut dest_optimized = dest_wasm.clone();
        dest_optimized.set_file_name(format!("{}-opt.wasm", contract_artifact_name));

        tracing::debug!(
            "Optimization level passed to wasm-opt: {}",
            self.optimization_level
        );

        let mut command = Command::new(self.wasm_opt_path.as_path());
        command
            .arg(dest_wasm.as_os_str())
            .arg(format!("-O{}", self.optimization_level))
            .arg("-o")
            .arg(dest_optimized.as_os_str())
            // the memory in our module is imported, `wasm-opt` needs to be told that
            // the memory is initialized to zeroes, otherwise it won't run the
            // memory-packing pre-pass.
            .arg("--zero-filled-memory");

        if self.keep_debug_symbols {
            command.arg("-g");
        }

        tracing::debug!("Invoking wasm-opt with {:?}", command);

        let output = command.output().map_err(|err| {
            anyhow::anyhow!(
                "Executing {} failed with {:?}",
                self.wasm_opt_path.display(),
                err
            )
        })?;

        if !output.status.success() {
            let err = str::from_utf8(&output.stderr)
                .expect("Cannot convert stderr output of wasm-opt to string")
                .trim();
            anyhow::bail!(
                "The wasm-opt optimization failed.\n\n\
                The error which wasm-opt returned was: \n{}",
                err
            );
        }

        if !dest_optimized.exists() {
            return Err(anyhow::anyhow!(
                "Optimization failed, optimized wasm output file `{}` not found.",
                dest_optimized.display()
            ))
        }

        let original_size = metadata(&dest_wasm)?.len() as f64 / 1000.0;
        let optimized_size = metadata(&dest_optimized)?.len() as f64 / 1000.0;

        // Overwrite existing destination wasm file with the optimised version
        std::fs::rename(&dest_optimized, &dest_wasm)?;
        Ok(OptimizationResult {
            dest_wasm: dest_wasm.clone(),
            original_size,
            optimized_size,
        })
    }

    /// The version number of the `wasm-opt` binary being executed.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Checks if the `wasm-opt` binary under `wasm_opt_path` returns a version
    /// compatible with `cargo-contract`.
    ///
    /// Currently this must be a version >= 99.
    fn check_wasm_opt_version_compatibility(wasm_opt_path: &Path) -> Result<u32> {
        let mut cmd_res = Command::new(wasm_opt_path).arg("--version").output();

        // The following condition is a workaround for a spurious CI failure:
        // ```
        // Executing `"/tmp/cargo-contract.test.GGnC0p/wasm-opt-mocked" --version` failed with
        // Os { code: 26, kind: ExecutableFileBusy, message: "Text file busy" }
        // ```
        if cmd_res.is_err() && format!("{:?}", cmd_res).contains("ExecutableFileBusy") {
            std::thread::sleep(std::time::Duration::from_secs(1));
            cmd_res = Command::new(wasm_opt_path).arg("--version").output();
        }

        let res = cmd_res.map_err(|err| {
            anyhow::anyhow!(
                "Executing `{:?} --version` failed with {:?}",
                wasm_opt_path.display(),
                err
            )
        })?;
        if !res.status.success() {
            let err = str::from_utf8(&res.stderr)
                .expect("Cannot convert stderr output of wasm-opt to string")
                .trim();
            anyhow::bail!(
                "Getting version information from wasm-opt failed.\n\
            The error which wasm-opt returned was: \n{}",
                err
            );
        }

        // ```sh
        // $ wasm-opt --version
        // wasm-opt version 99 (version_99-79-gc12cc3f50)
        // ```
        let github_note = "\n\n\
        If you tried installing from your system package manager the best\n\
        way forward is to download a recent binary release directly:\n\n\
        https://github.com/WebAssembly/binaryen/releases\n\n\
        Make sure that the `wasm-opt` file from that release is in your `PATH`.";
        let version_stdout = str::from_utf8(&res.stdout)
            .expect("Cannot convert stdout output of wasm-opt to string")
            .trim();
        let re = Regex::new(r"wasm-opt version (\d+)").expect("invalid regex");
        let captures = re.captures(version_stdout).ok_or_else(|| {
            anyhow::anyhow!(
                "Unable to extract version information from '{}'.\n\
                Your wasm-opt version is most probably too old. Make sure you use a version >= 99.{}",
                version_stdout,
                github_note,
            )
        })?;
        let version_number: u32 = captures
            .get(1) // first capture group is at index 1
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Unable to extract version number from '{:?}'",
                    version_stdout
                )
            })?
            .as_str()
            .parse()
            .map_err(|err| {
                anyhow::anyhow!(
                    "Parsing version number failed with '{:?}' for '{:?}'",
                    err,
                    version_stdout
                )
            })?;

        tracing::debug!(
            "The wasm-opt version output is '{}', which was parsed to '{}'",
            version_stdout,
            version_number
        );
        if version_number < 99 {
            anyhow::bail!(
                "Your wasm-opt version is {}, but we require a version >= 99.{}",
                version_number,
                github_note,
            );
        }

        Ok(version_number)
    }
}

#[cfg(feature = "test-ci-only")]
#[cfg(all(test, unix))]
mod tests_ci_only {
    use super::*;

    use crate::util::tests::{
        create_executable,
        with_tmp_dir,
        MockGuard,
    };

    /// Creates an executable `wasm-opt-mocked` file which outputs
    /// "wasm-opt version `version`".
    ///
    /// Returns the path to this file.
    ///
    /// Currently works only on `unix`.
    fn mock_wasm_opt_version(tmp_dir: &Path, version: &str) -> MockGuard {
        let path = tmp_dir.join("wasm-opt-mocked");
        let content = format!("#!/bin/sh\necho \"wasm-opt version {}\"", version);
        create_executable(&path, &content)
    }

    #[test]
    fn incompatible_wasm_opt_version_must_be_detected_if_built_from_repo() {
        with_tmp_dir(|path| {
            // given
            let path = mock_wasm_opt_version(path, "98 (version_13-79-gc12cc3f50)");

            // when
            let res = WasmOptHandler::check_wasm_opt_version_compatibility(&path);

            // then
            assert!(res.is_err());
            assert!(
                format!("{:?}", res).starts_with(
                    "Err(Your wasm-opt version is 98, but we require a version >= 99."
                ),
                "Expected a different output, found {:?}",
                res
            );

            Ok(())
        })
    }

    #[test]
    fn compatible_wasm_opt_version_must_be_detected_if_built_from_repo() {
        with_tmp_dir(|path| {
            // given
            let path = mock_wasm_opt_version(path, "99 (version_99-79-gc12cc3f50");

            // when
            let res = WasmOptHandler::check_wasm_opt_version_compatibility(&path);

            // then
            assert!(res.is_ok());

            Ok(())
        })
    }

    #[test]
    fn incompatible_wasm_opt_version_must_be_detected_if_installed_as_package() {
        with_tmp_dir(|path| {
            // given
            let path = mock_wasm_opt_version(path, "98");

            // when
            let res = WasmOptHandler::check_wasm_opt_version_compatibility(&path);

            // then
            assert!(res.is_err());

            // this println is here to debug a spuriously failing CI at the following assert.
            eprintln!("error: {:?}", res);
            assert!(format!("{:?}", res).starts_with(
                "Err(Your wasm-opt version is 98, but we require a version >= 99."
            ));

            Ok(())
        })
    }

    #[test]
    fn compatible_wasm_opt_version_must_be_detected_if_installed_as_package() {
        with_tmp_dir(|path| {
            // given
            let path = mock_wasm_opt_version(path, "99");

            // when
            let res = WasmOptHandler::check_wasm_opt_version_compatibility(&path);

            // then
            assert!(res.is_ok());

            Ok(())
        })
    }
}

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

use anyhow::Result;
use colored::Colorize;
use contract_build::{
    maybe_println,
    util,
    ManifestPath,
    Verbosity,
    VerbosityFlags,
};
use std::{
    convert::TryFrom,
    path::PathBuf,
};

/// Executes smart contract tests off-chain by delegating to `cargo test`.
#[derive(Debug, clap::Args)]
#[clap(name = "test")]
pub struct TestCommand {
    /// Path to the `Cargo.toml` of the contract to test.
    #[clap(long, value_parser)]
    manifest_path: Option<PathBuf>,
    #[clap(flatten)]
    verbosity: VerbosityFlags,
}

impl TestCommand {
    pub fn exec(&self) -> Result<TestResult> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;

        execute(&manifest_path, verbosity)
    }
}

/// Result of the test runs.
pub struct TestResult {
    /// The `cargo +nightly test` child process standard output stream buffer.
    pub stdout: Vec<u8>,
    /// The verbosity flags.
    pub verbosity: Verbosity,
}

impl TestResult {
    pub fn display(&self) -> Result<String> {
        Ok(String::from_utf8(self.stdout.clone())?)
    }
}

/// Executes `cargo +nightly test`.
pub(crate) fn execute(
    manifest_path: &ManifestPath,
    verbosity: Verbosity,
) -> Result<TestResult> {
    maybe_println!(
        verbosity,
        " {} {}",
        format!("[{}/{}]", 1, 1).bold(),
        "Running tests".bright_green().bold()
    );

    let stdout =
        util::invoke_cargo("test", [""], manifest_path.directory(), verbosity, vec![])?;

    Ok(TestResult { stdout, verbosity })
}

#[cfg(feature = "test-ci-only")]
#[cfg(test)]
mod tests_ci_only {
    use contract_build::{
        ManifestPath,
        Verbosity,
    };
    use regex::Regex;

    #[test]
    fn passing_tests_yield_stdout() {
        let tmp_dir = tempfile::Builder::new()
            .prefix("cargo-contract.test.")
            .tempdir()
            .expect("temporary directory creation failed");

        let project_name = "test_project";
        contract_build::new_contract_project(project_name, Some(&tmp_dir))
            .expect("new project creation failed");
        let working_dir = tmp_dir.path().join(project_name);
        let manifest_path = ManifestPath::new(working_dir.join("Cargo.toml"))
            .expect("invalid manifest path");

        let ok_output_pattern =
            Regex::new(r"test result: ok. \d+ passed; 0 failed; \d+ ignored")
                .expect("regex pattern compilation failed");

        let res = super::execute(&manifest_path, Verbosity::Default)
            .expect("test execution failed");

        assert!(ok_output_pattern.is_match(&String::from_utf8_lossy(&res.stdout)));
    }
}

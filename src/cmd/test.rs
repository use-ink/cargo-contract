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
    crate_metadata::get_cargo_workspace_members,
    maybe_println,
    util,
    workspace::ManifestPath,
    Verbosity,
    VerbosityFlags,
};
use anyhow::Result;
use colored::Colorize;
use std::{
    convert::TryFrom,
    path::PathBuf,
};

/// Executes smart contract tests off-chain by delegating to `cargo test`.
#[derive(Debug, clap::Args)]
#[clap(name = "test")]
pub struct TestCommand {
    /// Subcontract
    #[clap(long, short)]
    package: Option<String>,
    /// Path to the `Cargo.toml` of the contract to test.
    #[clap(long, parse(from_os_str))]
    manifest_path: Option<PathBuf>,
    /// Test all subcontracts in the workspace
    #[clap(long = "--all")]
    test_all: bool,
    #[clap(flatten)]
    verbosity: VerbosityFlags,
}

impl TestCommand {
    pub fn exec(&self) -> Result<Vec<TestResult>> {
        let manifest_path = match self.package.as_ref() {
            Some(package) => {
                let root_manifest_path =
                    ManifestPath::try_from(self.manifest_path.as_ref())?;
                root_manifest_path
                    .subcontract_manifest_path(package)
                    .unwrap_or_else(|| {
                        panic!(
                        "error: package ID specification `{}` did not match any packages",
                        package
                    )
                    })
            }
            None => ManifestPath::try_from(self.manifest_path.as_ref())?,
        };
        let verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;

        let mut ret = Vec::new();
        match self.test_all {
            true => {
                let workspace_members = get_cargo_workspace_members(&manifest_path)?;
                for package_id in workspace_members {
                    let manifest_path =
                        util::extract_subcontract_manifest_path(package_id)
                            .expect("Error extracting package manifest path");

                    ret.push(execute(&manifest_path, verbosity)?)
                }
            }
            false => ret.push(execute(&manifest_path, verbosity)?),
        }
        Ok(ret)
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
    util::assert_channel()?;

    maybe_println!(
        verbosity,
        " {} {}",
        format!("[{}/{}]", 1, 1).bold(),
        "Running tests".bright_green().bold()
    );

    let stdout =
        util::invoke_cargo("test", &[""], manifest_path.directory(), verbosity, vec![])?;

    Ok(TestResult { stdout, verbosity })
}

#[cfg(feature = "test-ci-only")]
#[cfg(test)]
mod tests_ci_only {
    use crate::{
        util::tests::with_new_contract_project,
        Verbosity,
    };
    use regex::Regex;

    #[test]
    fn passing_tests_yield_stdout() {
        with_new_contract_project(|manifest_path| {
            let ok_output_pattern =
                Regex::new(r"test result: ok. \d+ passed; 0 failed; \d+ ignored")
                    .expect("regex pattern compilation failed");

            let res = super::execute(&manifest_path, Verbosity::Default)
                .expect("test execution failed");

            assert!(ok_output_pattern.is_match(&String::from_utf8_lossy(&res.stdout)));

            Ok(())
        })
    }
}

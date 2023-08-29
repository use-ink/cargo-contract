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

use anyhow::{
    Context,
    Result,
};
use colored::Colorize;
use contract_build::{
    execute,
    verbose_eprintln,
    BuildArtifacts,
    BuildInfo,
    ExecuteArgs,
    ManifestPath,
    Verbosity,
    VerbosityFlags,
};
use contract_metadata::{
    ContractMetadata,
    SourceWasm,
};

use std::{
    fs::File,
    path::PathBuf,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Checks if a contract in the given workspace matches that of a reference contract.
#[derive(Debug, clap::Args)]
#[clap(name = "verify")]
pub struct VerifyCommand {
    /// Path to the `Cargo.toml` of the contract to verify.
    #[clap(long, value_parser)]
    manifest_path: Option<PathBuf>,
    /// The reference Wasm contract (`*.contract`) that the workspace will be checked
    /// against.
    contract: PathBuf,
    /// Denotes if output should be printed to stdout.
    #[clap(flatten)]
    verbosity: VerbosityFlags,
}

impl VerifyCommand {
    pub fn run(&self) -> Result<()> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let verbosity: Verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;

        // 1. Read the given metadata, and pull out the `BuildInfo`
        let path = &self.contract;
        let file = File::open(path)
            .context(format!("Failed to open contract bundle {}", path.display()))?;

        let metadata: ContractMetadata = serde_json::from_reader(&file).context(
            format!("Failed to deserialize contract bundle {}", path.display()),
        )?;
        let build_info = if let Some(info) = metadata.source.build_info {
            info
        } else {
            anyhow::bail!(
                "\nThe metadata does not contain any build information which can be used to \
                verify a contract."
                .to_string()
                .bright_yellow()
            )
        };

        let build_info: BuildInfo =
            serde_json::from_value(build_info.into()).context(format!(
                "Failed to deserialize the build info from {}",
                path.display()
            ))?;

        tracing::debug!(
            "Parsed the following build info from the metadata: {:?}",
            &build_info,
        );

        // 2. Check that the build info from the metadata matches our current setup.
        let expected_rust_toolchain = build_info.rust_toolchain;
        let rust_toolchain = contract_build::util::rust_toolchain()
            .expect("`rustc` always has a version associated with it.");

        let rustc_matches = rust_toolchain == expected_rust_toolchain;
        let mismatched_rustc = format!(
            "\nYou are trying to `verify` a contract using the `{rust_toolchain}` toolchain.\n\
             However, the original contract was built using `{expected_rust_toolchain}`. Please\n\
             install the correct toolchain (`rustup install {expected_rust_toolchain}`) and\n\
             re-run the `verify` command.",);
        anyhow::ensure!(rustc_matches, mismatched_rustc.bright_yellow());

        let expected_cargo_contract_version = build_info.cargo_contract_version;
        let cargo_contract_version = semver::Version::parse(VERSION)?;

        // Note, assuming both versions of `cargo-contract` were installed with the same
        // lockfile (e.g `--locked`) then the versions of `wasm-opt` should also
        // match.
        let cargo_contract_matches =
            cargo_contract_version == expected_cargo_contract_version;
        let mismatched_cargo_contract = format!(
            "\nYou are trying to `verify` a contract using `cargo-contract` version \
            `{cargo_contract_version}`.\n\
             However, the original contract was built using `cargo-contract` version \
             `{expected_cargo_contract_version}`.\n\
             Please install the matching version and re-run the `verify` command.",
        );
        anyhow::ensure!(
            cargo_contract_matches,
            mismatched_cargo_contract.bright_yellow()
        );

        // 3. Call `cargo contract build` with the `BuildInfo` from the metadata.
        let args = ExecuteArgs {
            manifest_path: manifest_path.clone(),
            verbosity,
            build_mode: build_info.build_mode,
            network: Default::default(),
            build_artifact: BuildArtifacts::CodeOnly,
            unstable_flags: Default::default(),
            optimization_passes: Some(build_info.wasm_opt_settings.optimization_passes),
            keep_debug_symbols: build_info.wasm_opt_settings.keep_debug_symbols,
            dylint: false,
            output_type: Default::default(),
            ..Default::default()
        };

        let build_result = execute(args)?;

        // 4. Grab the built Wasm contract and compare it with the Wasm from the metadata.
        let reference_wasm = if let Some(wasm) = metadata.source.wasm {
            wasm
        } else {
            anyhow::bail!(
                "\nThe metadata for the reference contract does not contain a Wasm binary,\n\
                therefore we are unable to verify the contract."
                .to_string()
                .bright_yellow()
            )
        };

        let built_wasm_path = if let Some(wasm) = build_result.dest_wasm {
            wasm
        } else {
            // Since we're building the contract ourselves this should always be
            // populated, but we'll bail out here just in case.
            anyhow::bail!(
                "\nThe metadata for the workspace contract does not contain a Wasm binary,\n\
                therefore we are unable to verify the contract."
                .to_string()
                .bright_yellow()
            )
        };

        let fs_wasm = std::fs::read(built_wasm_path)?;
        let built_wasm = SourceWasm::new(fs_wasm);

        if reference_wasm != built_wasm {
            tracing::debug!(
                "Expected Wasm Binary '{}'\n\nGot Wasm Binary `{}`",
                &reference_wasm,
                &built_wasm
            );

            anyhow::bail!(format!(
                "\nFailed to verify the authenticity of {} contract againt the workspace \n\
                found at {}.",
                format!("`{}`", metadata.contract.name).bright_white(),
                format!("{:?}", manifest_path.as_ref()).bright_white()).bright_red()
            );
        }

        verbose_eprintln!(
            verbosity,
            " \n{} {}",
            "Successfully verified contract".bright_green().bold(),
            format!("`{}`!", &metadata.contract.name).bold(),
        );

        Ok(())
    }
}

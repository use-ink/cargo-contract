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
mod tests;

use contract_build::{
    maybe_println,
    util,
    validate_wasm,
    wasm_opt::WasmOptHandler,
    workspace::{
        Manifest,
        ManifestPath,
        Profile,
        Workspace,
    },
    BuildArtifacts,
    BuildMode,
    BuildResult,
    Network,
    OptimizationPasses,
    OptimizationResult,
    OutputType,
    UnstableFlags,
    UnstableOptions,
    Verbosity,
    VerbosityFlags,
};
use anyhow::{
    Context,
    Result,
};
use colored::Colorize;
use parity_wasm::elements::{
    External,
    Internal,
    MemoryType,
    Module,
    Section,
};
use semver::Version;
use std::{
    convert::TryFrom,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
    str,
};

/// This is the maximum number of pages available for a contract to allocate.
const MAX_MEMORY_PAGES: u32 = 16;

/// Arguments to use when executing `build` or `check` commands.
#[derive(Default)]
pub(crate) struct ExecuteArgs {
    /// The location of the Cargo manifest (`Cargo.toml`) file to use.
    pub manifest_path: ManifestPath,
    pub verbosity: Verbosity,
    pub build_mode: BuildMode,
    pub network: Network,
    pub build_artifact: BuildArtifacts,
    pub unstable_flags: UnstableFlags,
    pub optimization_passes: OptimizationPasses,
    pub keep_debug_symbols: bool,
    pub skip_linting: bool,
    pub output_type: OutputType,
}

/// Executes build of the smart contract which produces a Wasm binary that is ready for deploying.
///
/// It does so by invoking `cargo build` and then post processing the final binary.
#[derive(Debug, clap::Args)]
#[clap(name = "build")]
pub struct BuildCommand {
    /// Path to the `Cargo.toml` of the contract to build
    #[clap(long, value_parser)]
    manifest_path: Option<PathBuf>,
    /// By default the contract is compiled with debug functionality
    /// included. This enables the contract to output debug messages,
    /// but increases the contract size and the amount of gas used.
    ///
    /// A production contract should always be build in `release` mode!
    /// Then no debug functionality is compiled into the contract.
    #[clap(long = "release")]
    build_release: bool,
    /// Build offline
    #[clap(long = "offline")]
    build_offline: bool,
    /// Skips linting checks during the build process
    #[clap(long)]
    skip_linting: bool,
    /// Which build artifacts to generate.
    ///
    /// - `all`: Generate the Wasm, the metadata and a bundled `<name>.contract` file.
    ///
    /// - `code-only`: Only the Wasm is created, generation of metadata and a bundled
    ///   `<name>.contract` file is skipped.
    ///
    /// - `check-only`: No artifacts produced: runs the `cargo check` command for the Wasm target,
    ///    only checks for compilation errors.
    #[clap(long = "generate", value_enum, default_value = "all")]
    build_artifact: BuildArtifacts,
    #[clap(flatten)]
    verbosity: VerbosityFlags,
    #[clap(flatten)]
    unstable_options: UnstableOptions,
    /// Number of optimization passes, passed as an argument to `wasm-opt`.
    ///
    /// - `0`: execute no optimization passes
    ///
    /// - `1`: execute 1 optimization pass (quick & useful opts, useful for iteration builds)
    ///
    /// - `2`, execute 2 optimization passes (most opts, generally gets most perf)
    ///
    /// - `3`, execute 3 optimization passes (spends potentially a lot of time optimizing)
    ///
    /// - `4`, execute 4 optimization passes (also flatten the IR, which can take a lot more time and memory
    /// but is useful on more nested / complex / less-optimized input)
    ///
    /// - `s`, execute default optimization passes, focusing on code size
    ///
    /// - `z`, execute default optimization passes, super-focusing on code size
    ///
    /// - The default value is `z`
    ///
    /// - It is possible to define the number of optimization passes in the
    ///   `[package.metadata.contract]` of your `Cargo.toml` as e.g. `optimization-passes = "3"`.
    ///   The CLI argument always takes precedence over the profile value.
    #[clap(long)]
    optimization_passes: Option<OptimizationPasses>,
    /// Do not remove symbols (Wasm name section) when optimizing.
    ///
    /// This is useful if one wants to analyze or debug the optimized binary.
    #[clap(long)]
    keep_debug_symbols: bool,

    /// Export the build output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

impl BuildCommand {
    pub fn exec(&self) -> Result<BuildResult> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let unstable_flags: UnstableFlags =
            TryFrom::<&UnstableOptions>::try_from(&self.unstable_options)?;
        let mut verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;

        // The CLI flag `optimization-passes` overwrites optimization passes which are
        // potentially defined in the `Cargo.toml` profile.
        let optimization_passes = match self.optimization_passes {
            Some(opt_passes) => opt_passes,
            None => {
                let mut manifest = Manifest::new(manifest_path.clone())?;
                match manifest.get_profile_optimization_passes() {
                    // if no setting is found, neither on the cli nor in the profile,
                    // then we use the default
                    None => OptimizationPasses::default(),
                    Some(opt_passes) => opt_passes,
                }
            }
        };

        let build_mode = match self.build_release {
            true => BuildMode::Release,
            false => BuildMode::Debug,
        };

        let network = match self.build_offline {
            true => Network::Offline,
            false => Network::Online,
        };

        // The invocation of `cargo dylint` requires network access, so in offline mode the linting
        // step must be skipped otherwise the build can fail.
        let skip_linting = self.skip_linting || matches!(network, Network::Offline);

        let output_type = match self.output_json {
            true => OutputType::Json,
            false => OutputType::HumanReadable,
        };

        // We want to ensure that the only thing in `STDOUT` is our JSON formatted string.
        if matches!(output_type, OutputType::Json) {
            verbosity = Verbosity::Quiet;
        }

        let args = ExecuteArgs {
            manifest_path,
            verbosity,
            build_mode,
            network,
            build_artifact: self.build_artifact,
            unstable_flags,
            optimization_passes,
            keep_debug_symbols: self.keep_debug_symbols,
            skip_linting,
            output_type,
        };

        execute(args)
    }
}

#[derive(Debug, clap::Args)]
#[clap(name = "check")]
pub struct CheckCommand {
    /// Path to the `Cargo.toml` of the contract to build
    #[clap(long, value_parser)]
    manifest_path: Option<PathBuf>,
    #[clap(flatten)]
    verbosity: VerbosityFlags,
    #[clap(flatten)]
    unstable_options: UnstableOptions,
}

impl CheckCommand {
    pub fn exec(&self) -> Result<BuildResult> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let unstable_flags: UnstableFlags =
            TryFrom::<&UnstableOptions>::try_from(&self.unstable_options)?;
        let verbosity: Verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;

        let args = ExecuteArgs {
            manifest_path,
            verbosity,
            build_mode: BuildMode::Debug,
            network: Network::default(),
            build_artifact: BuildArtifacts::CheckOnly,
            unstable_flags,
            optimization_passes: OptimizationPasses::Zero,
            keep_debug_symbols: false,
            skip_linting: false,
            output_type: OutputType::default(),
        };

        execute(args)
    }
}
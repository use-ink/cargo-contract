// Copyright (C) Use Ink (UK) Ltd.
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
use contract_build::{
    BuildArtifacts,
    BuildMode,
    BuildResult,
    ExecuteArgs,
    Features,
    ImageVariant,
    ManifestPath,
    MetadataSpec,
    Network,
    OutputType,
    UnstableFlags,
    UnstableOptions,
    Verbosity,
    VerbosityFlags,
};
use std::{
    convert::TryFrom,
    path::PathBuf,
};

/// Executes build of the smart contract which produces a PolkaVM binary that is ready for
/// deploying.
///
/// It does so by invoking `cargo build` and then post-processing the final binary.
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
    /// Which build artifacts to generate.
    ///
    /// - `all`: Generate the contract binary (`<name>.polkavm`), the metadata and a
    ///   bundled `<name>.contract` file.
    ///
    /// - `code-only`: Only the contract binary (`<name>.polkavm`) is created, generation
    ///   of metadata and a bundled `<name>.contract` file is skipped.
    ///
    /// - `check-only`: No artifacts produced: runs the `cargo check` command for the
    ///   PolkaVM target, only checks for compilation errors.
    #[clap(long = "generate", value_enum, default_value = "all")]
    build_artifact: BuildArtifacts,
    #[clap(flatten)]
    features: Features,
    #[clap(flatten)]
    verbosity: VerbosityFlags,
    #[clap(flatten)]
    unstable_options: UnstableOptions,
    /// todo update comment
    /// Do not remove symbols (Wasm name section) when optimizing.
    ///
    /// This is useful if one wants to analyze or debug the optimized binary.
    #[clap(long)]
    keep_debug_symbols: bool,
    /// Export the build output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
    /// Executes the build inside a docker container to produce a verifiable bundle.
    /// Requires docker daemon running.
    #[clap(long, default_value_t = false)]
    verifiable: bool,
    /// Specify a custom image for the verifiable build
    #[clap(long, default_value = None)]
    image: Option<String>,
    /// Which specification to use for contract metadata.
    #[clap(long, default_value = "ink")]
    metadata: MetadataSpec,
}

impl BuildCommand {
    pub fn exec(&self) -> Result<BuildResult> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let unstable_flags: UnstableFlags =
            TryFrom::<&UnstableOptions>::try_from(&self.unstable_options)?;
        let verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;

        let build_mode = if self.verifiable {
            BuildMode::Verifiable
        } else {
            match self.build_release {
                true => BuildMode::Release,
                false => BuildMode::Debug,
            }
        };

        let network = match self.build_offline {
            true => Network::Offline,
            false => Network::Online,
        };

        let output_type = match self.output_json {
            true => OutputType::Json,
            false => OutputType::HumanReadable,
        };

        if self.image.is_some() && build_mode != BuildMode::Verifiable {
            anyhow::bail!("--image flag can only be used with verifiable builds!");
        }

        let image = match &self.image {
            Some(i) => ImageVariant::Custom(i.clone()),
            None => ImageVariant::Default,
        };

        let args = ExecuteArgs {
            manifest_path,
            verbosity,
            build_mode,
            features: self.features.clone(),
            network,
            build_artifact: self.build_artifact,
            unstable_flags,
            keep_debug_symbols: self.keep_debug_symbols,
            extra_lints: false,
            output_type,
            image,
            metadata_spec: self.metadata,
        };
        contract_build::execute(args)
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
}

impl CheckCommand {
    pub fn exec(&self) -> Result<BuildResult> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let verbosity: Verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;

        let args = ExecuteArgs {
            manifest_path,
            verbosity,
            build_mode: BuildMode::Debug,
            features: Default::default(),
            network: Network::default(),
            build_artifact: BuildArtifacts::CheckOnly,
            unstable_flags: Default::default(),
            keep_debug_symbols: false,
            extra_lints: false,
            output_type: OutputType::default(),
            image: ImageVariant::Default,
            metadata_spec: Default::default(),
        };

        contract_build::execute(args)
    }
}

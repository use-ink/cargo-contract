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
use clap::Args;
use std::{
    convert::TryFrom,
    fmt,
};

#[derive(Default, Clone, Debug, Args)]
pub struct VerbosityFlags {
    /// No output printed to stdout
    #[clap(long)]
    quiet: bool,
    /// Use verbose output
    #[clap(long)]
    verbose: bool,
}

impl TryFrom<&VerbosityFlags> for Verbosity {
    type Error = anyhow::Error;

    fn try_from(value: &VerbosityFlags) -> Result<Self, Self::Error> {
        match (value.quiet, value.verbose) {
            (false, false) => Ok(Verbosity::Default),
            (true, false) => Ok(Verbosity::Quiet),
            (false, true) => Ok(Verbosity::Verbose),
            (true, true) => anyhow::bail!("Cannot pass both --quiet and --verbose flags"),
        }
    }
}

/// Denotes if output should be printed to stdout.
#[derive(Clone, Copy, serde::Serialize, Eq, PartialEq)]
pub enum Verbosity {
    /// Use default output
    Default,
    /// No output printed to stdout
    Quiet,
    /// Use verbose output
    Verbose,
}

impl Default for Verbosity {
    fn default() -> Self {
        Verbosity::Default
    }
}

impl Verbosity {
    /// Returns `true` if output should be printed (i.e. verbose output is set).
    pub fn is_verbose(&self) -> bool {
        match self {
            Verbosity::Quiet => false,
            Verbosity::Default | Verbosity::Verbose => true,
        }
    }
}

/// Use network connection to build contracts and generate metadata or use cached dependencies only.
#[derive(Eq, PartialEq, Copy, Clone, Debug, serde::Serialize)]
pub enum Network {
    /// Use network
    Online,
    /// Use cached dependencies.
    Offline,
}

impl Default for Network {
    fn default() -> Network {
        Network::Online
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Online => write!(f, ""),
            Self::Offline => write!(f, "--offline"),
        }
    }
}

/// Describes which artifacts to generate
#[derive(Copy, Clone, Eq, PartialEq, Debug, clap::ValueEnum, serde::Serialize)]
#[clap(name = "build-artifacts")]
pub enum BuildArtifacts {
    /// Generate the Wasm, the metadata and a bundled `<name>.contract` file
    #[clap(name = "all")]
    All,
    /// Only the Wasm is created, generation of metadata and a bundled `<name>.contract` file is
    /// skipped
    #[clap(name = "code-only")]
    CodeOnly,
    /// No artifacts produced: runs the `cargo check` command for the Wasm target, only checks for
    /// compilation errors.
    #[clap(name = "check-only")]
    CheckOnly,
}

impl BuildArtifacts {
    /// Returns the number of steps required to complete a build artifact.
    /// Used as output on the cli.
    pub fn steps(&self) -> BuildSteps {
        match self {
            BuildArtifacts::All => BuildSteps::new(5),
            BuildArtifacts::CodeOnly => BuildSteps::new(4),
            BuildArtifacts::CheckOnly => BuildSteps::new(1),
        }
    }
}

impl Default for BuildArtifacts {
    fn default() -> Self {
        BuildArtifacts::All
    }
}

/// Track and display the current and total number of steps.
#[derive(Debug, Clone, Copy)]
pub struct BuildSteps {
    pub current_step: usize,
    pub total_steps: usize,
}

impl BuildSteps {
    pub fn new(total_steps: usize) -> Self {
        Self {
            current_step: 1,
            total_steps,
        }
    }

    pub fn increment_current(&mut self) {
        self.current_step += 1;
    }
}

impl fmt::Display for BuildSteps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}/{}]", self.current_step, self.total_steps)
    }
}

/// The mode to build the contract in.
#[derive(Eq, PartialEq, Copy, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum BuildMode {
    /// Functionality to output debug messages is build into the contract.
    Debug,
    /// The contract is build without any debugging functionality.
    Release,
}

impl Default for BuildMode {
    fn default() -> BuildMode {
        BuildMode::Debug
    }
}

impl fmt::Display for BuildMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Debug => write!(f, "debug"),
            Self::Release => write!(f, "release"),
        }
    }
}

/// The type of output to display at the end of a build.
pub enum OutputType {
    /// Output build results in a human readable format.
    HumanReadable,
    /// Output the build results JSON formatted.
    Json,
}

impl Default for OutputType {
    fn default() -> Self {
        OutputType::HumanReadable
    }
}

#[derive(Default, Clone, Debug, Args)]
pub struct UnstableOptions {
    /// Use the original manifest (Cargo.toml), do not modify for build optimizations
    #[clap(long = "unstable-options", short = 'Z', number_of_values = 1)]
    options: Vec<String>,
}

#[derive(Clone, Default)]
pub struct UnstableFlags {
    pub original_manifest: bool,
}

impl TryFrom<&UnstableOptions> for UnstableFlags {
    type Error = anyhow::Error;

    fn try_from(value: &UnstableOptions) -> Result<Self, Self::Error> {
        let valid_flags = ["original-manifest"];
        let invalid_flags = value
            .options
            .iter()
            .filter(|o| !valid_flags.contains(&o.as_str()))
            .collect::<Vec<_>>();
        if !invalid_flags.is_empty() {
            anyhow::bail!("Unknown unstable-options {:?}", invalid_flags)
        }
        Ok(UnstableFlags {
            original_manifest: value.options.contains(&"original-manifest".to_owned()),
        })
    }
}

/// Define the standard `cargo` features args to be passed through.
#[derive(Default, Clone, Debug, Args)]
pub struct Features {
    /// Space or comma separated list of features to activate
    #[clap(long)]
    features: Vec<String>,
}

impl Features {
    /// Appends a feature.
    pub fn push(&mut self, feature: &str) {
        self.features.push(feature.to_owned())
    }

    /// Appends the raw features args to pass through to the `cargo` invocation.
    pub fn append_to_args<'a>(&'a self, args: &mut Vec<&'a str>) {
        if !self.features.is_empty() {
            args.push("--features");
            for feature in &self.features {
                args.push(feature)
            }
        }
    }
}

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

use crate::CrateMetadata;
use anyhow::Result;
use clap::Args;
use std::{
    convert::TryFrom,
    fmt,
    fs,
    fs::File,
    io::Write,
    path::Path,
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
#[derive(
    Clone, Copy, Default, serde::Serialize, serde::Deserialize, Eq, PartialEq, Debug,
)]
pub enum Verbosity {
    /// Use default output
    #[default]
    Default,
    /// No output printed to stdout
    Quiet,
    /// Use verbose output
    Verbose,
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

/// Use network connection to build contracts and generate metadata or use cached
/// dependencies only.
#[derive(Eq, PartialEq, Copy, Clone, Debug, Default, serde::Serialize)]
pub enum Network {
    /// Use network
    #[default]
    Online,
    /// Use cached dependencies.
    Offline,
}

impl Network {
    /// If `Network::Offline` append the `--offline` flag for cargo invocations.
    pub fn append_to_args(&self, args: &mut Vec<String>) {
        match self {
            Self::Online => (),
            Self::Offline => args.push("--offline".to_owned()),
        }
    }
}

/// Describes which artifacts to generate
#[derive(
    Copy,
    Clone,
    Default,
    Eq,
    PartialEq,
    Debug,
    clap::ValueEnum,
    serde::Serialize,
    serde::Deserialize,
)]
#[clap(name = "build-artifacts")]
pub enum BuildArtifacts {
    /// Generate the Wasm, the metadata and a bundled `<name>.contract` file
    #[clap(name = "all")]
    #[default]
    All,
    /// Only the Wasm is created, generation of metadata and a bundled `<name>.contract`
    /// file is skipped
    #[clap(name = "code-only")]
    CodeOnly,
    /// No artifacts produced: runs the `cargo check` command for the Wasm target, only
    /// checks for compilation errors.
    #[clap(name = "check-only")]
    CheckOnly,
}

impl BuildArtifacts {
    /// Returns the number of steps required to complete a build artifact.
    /// Used as output on the cli.
    pub fn steps(&self) -> usize {
        match self {
            BuildArtifacts::All => 5,
            BuildArtifacts::CodeOnly => 4,
            BuildArtifacts::CheckOnly => 1,
        }
    }
}

/// The list of targets that ink! supports.
#[derive(
    Eq,
    PartialEq,
    Copy,
    Clone,
    Debug,
    Default,
    clap::ValueEnum,
    serde::Serialize,
    serde::Deserialize,
    strum::EnumIter,
)]
pub enum Target {
    /// WebAssembly
    #[clap(name = "wasm")]
    #[default]
    Wasm,
    /// RISC-V: Experimental
    #[clap(name = "riscv")]
    RiscV,
}

impl Target {
    /// The target string to be passed to rustc in order to build for this target.
    pub fn llvm_target(&self, crate_metadata: &CrateMetadata) -> String {
        match self {
            Self::Wasm => "wasm32-unknown-unknown".to_string(),
            Self::RiscV => {
                // Instead of a target literal we use a JSON file with a more complex
                // target configuration here. The path to the file is passed for the
                // `rustc --target` argument. We write this file to the `target/` folder.
                let target_dir = crate_metadata.target_directory.to_string_lossy();
                let path =
                    format!("{}/riscv32emac-unknown-none-polkavm.json", target_dir);
                if !Path::exists(Path::new(&path)) {
                    fs::create_dir_all(&crate_metadata.target_directory).unwrap_or_else(
                        |e| {
                            panic!(
                                "unable to create target dir {:?}: {:?}",
                                target_dir, e
                            )
                        },
                    );
                    let mut file = File::create(&path).unwrap();
                    let config = include_str!("../riscv32emac-unknown-none-polkavm.json");
                    file.write_all(config.as_bytes()).unwrap();
                }
                path
            }
        }
    }

    /// The name used for the target folder inside the `target/` folder.
    pub fn llvm_target_alias(&self) -> &'static str {
        match self {
            Self::Wasm => "wasm32-unknown-unknown",
            Self::RiscV => "riscv32emac-unknown-none-polkavm",
        }
    }

    /// Target specific flags to be set to `CARGO_ENCODED_RUSTFLAGS` while building.
    pub fn rustflags(&self) -> Option<&'static str> {
        match self {
            Self::Wasm => Some("-Clink-arg=-zstack-size=65536\x1f-Clink-arg=--import-memory\x1f-Ctarget-cpu=mvp"),
            // Substrate has the `cfg` `substrate_runtime` to distinguish if e.g. `sp-io`
            // is being build for `std` or for a Wasm/RISC-V runtime.
            Self::RiscV => Some("--cfg\x1fsubstrate_runtime"),
        }
    }

    /// The file extension that is used by rustc when outputting the binary.
    pub fn source_extension(&self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::RiscV => "",
        }
    }

    // The file extension that is used to store the post processed binary.
    pub fn dest_extension(&self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::RiscV => "riscv",
        }
    }
}

/// The mode to build the contract in.
#[derive(
    Eq, PartialEq, Copy, Clone, Debug, Default, serde::Serialize, serde::Deserialize,
)]
pub enum BuildMode {
    /// Functionality to output debug messages is build into the contract.
    #[default]
    Debug,
    /// The contract is built without any debugging functionality.
    Release,
    /// the contract is built in release mode and in a deterministic environment.
    Verifiable,
}

impl fmt::Display for BuildMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Debug => write!(f, "debug"),
            Self::Release => write!(f, "release"),
            Self::Verifiable => write!(f, "verifiable"),
        }
    }
}

/// The type of output to display at the end of a build.
#[derive(Clone, Debug, Default)]
pub enum OutputType {
    /// Output build results in a human readable format.
    #[default]
    HumanReadable,
    /// Output the build results JSON formatted.
    Json,
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
    #[clap(long, value_delimiter = ',')]
    features: Vec<String>,
}

impl Features {
    /// Appends a feature.
    pub fn push(&mut self, feature: &str) {
        self.features.push(feature.to_owned())
    }

    /// Appends the raw features args to pass through to the `cargo` invocation.
    pub fn append_to_args(&self, args: &mut Vec<String>) {
        if !self.features.is_empty() {
            args.push("--features".to_string());
            let features = if self.features.len() == 1 {
                self.features[0].clone()
            } else {
                self.features.join(",")
            };
            args.push(features);
        }
    }
}

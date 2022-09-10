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

#![deny(unused_crate_dependencies)]

mod cmd;
mod crate_metadata;
mod util;
mod validate_wasm;
mod wasm_opt;
mod workspace;

use self::{
    cmd::{
        metadata::MetadataResult,
        BuildCommand,
        CallCommand,
        CheckCommand,
        DecodeCommand,
        InstantiateCommand,
        TestCommand,
        UploadCommand,
    },
    util::DEFAULT_KEY_COL_WIDTH,
    workspace::ManifestPath,
};

use std::{
    convert::TryFrom,
    fmt::{
        Display,
        Formatter,
        Result as DisplayResult,
    },
    path::PathBuf,
    str::FromStr,
};

use anyhow::{
    Error,
    Result,
};
use clap::{
    AppSettings,
    Args,
    Parser,
    Subcommand,
};
use colored::Colorize;

// These crates are only used when we run integration tests `--features integration-tests`. However
// since we can't have optional `dev-dependencies` we pretend to use them during normal test runs
// in order to satisfy the `unused_crate_dependencies` lint.
#[cfg(test)]
use assert_cmd as _;

#[cfg(test)]
use predicates as _;

#[derive(Debug, Parser)]
#[clap(bin_name = "cargo")]
#[clap(version = env!("CARGO_CONTRACT_CLI_IMPL_VERSION"))]
pub(crate) enum Opts {
    /// Utilities to develop Wasm smart contracts.
    #[clap(name = "contract")]
    #[clap(version = env!("CARGO_CONTRACT_CLI_IMPL_VERSION"))]
    #[clap(setting = AppSettings::DeriveDisplayOrder)]
    Contract(ContractArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ContractArgs {
    #[clap(subcommand)]
    cmd: Command,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct HexData(pub Vec<u8>);

impl FromStr for HexData {
    type Err = hex::FromHexError;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        hex::decode(input).map(HexData)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptimizationPasses {
    Zero,
    One,
    Two,
    Three,
    Four,
    S,
    Z,
}

impl Display for OptimizationPasses {
    fn fmt(&self, f: &mut Formatter<'_>) -> DisplayResult {
        let out = match self {
            OptimizationPasses::Zero => "0",
            OptimizationPasses::One => "1",
            OptimizationPasses::Two => "2",
            OptimizationPasses::Three => "3",
            OptimizationPasses::Four => "4",
            OptimizationPasses::S => "s",
            OptimizationPasses::Z => "z",
        };
        write!(f, "{}", out)
    }
}

impl Default for OptimizationPasses {
    fn default() -> OptimizationPasses {
        OptimizationPasses::Z
    }
}

impl FromStr for OptimizationPasses {
    type Err = Error;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        // We need to replace " here, since the input string could come
        // from either the CLI or the `Cargo.toml` profile section.
        // If it is from the profile it could e.g. be "3" or 3.
        let normalized_input = input.replace('"', "").to_lowercase();
        match normalized_input.as_str() {
            "0" => Ok(OptimizationPasses::Zero),
            "1" => Ok(OptimizationPasses::One),
            "2" => Ok(OptimizationPasses::Two),
            "3" => Ok(OptimizationPasses::Three),
            "4" => Ok(OptimizationPasses::Four),
            "s" => Ok(OptimizationPasses::S),
            "z" => Ok(OptimizationPasses::Z),
            _ => anyhow::bail!("Unknown optimization passes for option {}", input),
        }
    }
}

impl From<String> for OptimizationPasses {
    fn from(str: String) -> Self {
        OptimizationPasses::from_str(&str).expect("conversion failed")
    }
}

#[derive(Default, Clone, Debug, Args)]
pub struct VerbosityFlags {
    /// No output printed to stdout
    #[clap(long)]
    quiet: bool,
    /// Use verbose output
    #[clap(long)]
    verbose: bool,
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
    pub(crate) fn is_verbose(&self) -> bool {
        match self {
            Verbosity::Quiet => false,
            Verbosity::Default | Verbosity::Verbose => true,
        }
    }
}

impl TryFrom<&VerbosityFlags> for Verbosity {
    type Error = Error;

    fn try_from(value: &VerbosityFlags) -> Result<Self, Self::Error> {
        match (value.quiet, value.verbose) {
            (false, false) => Ok(Verbosity::Default),
            (true, false) => Ok(Verbosity::Quiet),
            (false, true) => Ok(Verbosity::Verbose),
            (true, true) => anyhow::bail!("Cannot pass both --quiet and --verbose flags"),
        }
    }
}

#[derive(Default, Clone, Debug, Args)]
struct UnstableOptions {
    /// Use the original manifest (Cargo.toml), do not modify for build optimizations
    #[clap(long = "unstable-options", short = 'Z', number_of_values = 1)]
    options: Vec<String>,
}

#[derive(Clone, Default)]
struct UnstableFlags {
    original_manifest: bool,
}

impl TryFrom<&UnstableOptions> for UnstableFlags {
    type Error = Error;

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

/// Describes which artifacts to generate
#[derive(Copy, Clone, Eq, PartialEq, Debug, clap::ArgEnum, serde::Serialize)]
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
    pub fn steps(&self) -> usize {
        match self {
            BuildArtifacts::All => 6,
            BuildArtifacts::CodeOnly => 4,
            BuildArtifacts::CheckOnly => 2,
        }
    }
}

impl Default for BuildArtifacts {
    fn default() -> Self {
        BuildArtifacts::All
    }
}

/// The mode to build the contract in.
#[derive(Eq, PartialEq, Copy, Clone, Debug, serde::Serialize)]
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

impl Display for BuildMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> DisplayResult {
        match self {
            Self::Debug => write!(f, "debug"),
            Self::Release => write!(f, "release"),
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

impl Display for Network {
    fn fmt(&self, f: &mut Formatter<'_>) -> DisplayResult {
        match self {
            Self::Online => write!(f, ""),
            Self::Offline => write!(f, "--offline"),
        }
    }
}

/// The type of output to display at the end of a build.
#[derive(Clone)]
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

/// Result of the metadata generation process.
#[derive(serde::Serialize)]
pub struct BuildResult {
    /// Path to the resulting Wasm file.
    pub dest_wasm: Option<PathBuf>,
    /// Result of the metadata generation.
    pub metadata_result: Option<MetadataResult>,
    /// Path to the directory where output files are written to.
    pub target_directory: PathBuf,
    /// If existent the result of the optimization.
    pub optimization_result: Option<OptimizationResult>,
    /// The mode to build the contract in.
    pub build_mode: BuildMode,
    /// Which build artifacts were generated.
    pub build_artifact: BuildArtifacts,
    /// The verbosity flags.
    pub verbosity: Verbosity,
    /// The type of formatting to use for the build output.
    #[serde(skip_serializing)]
    pub output_type: OutputType,
}

/// Result of the optimization process.
#[derive(serde::Serialize)]
pub struct OptimizationResult {
    /// The path of the optimized Wasm file.
    pub dest_wasm: PathBuf,
    /// The original Wasm size.
    pub original_size: f64,
    /// The Wasm size after optimizations have been applied.
    pub optimized_size: f64,
}

impl BuildResult {
    pub fn display(&self) -> String {
        let optimization = self.display_optimization();
        let size_diff = format!(
            "\nOriginal wasm size: {}, Optimized: {}\n\n",
            format!("{:.1}K", optimization.0).bold(),
            format!("{:.1}K", optimization.1).bold(),
        );
        debug_assert!(
            optimization.1 > 0.0,
            "optimized file size must be greater 0"
        );

        let build_mode = format!(
            "The contract was built in {} mode.\n\n",
            format!("{}", self.build_mode).to_uppercase().bold(),
        );

        if self.build_artifact == BuildArtifacts::CodeOnly {
            let out = format!(
                "{}{}Your contract's code is ready. You can find it here:\n{}",
                size_diff,
                build_mode,
                self.dest_wasm
                    .as_ref()
                    .expect("wasm path must exist")
                    .display()
                    .to_string()
                    .bold()
            );
            return out
        };

        let mut out = format!(
            "{}{}Your contract artifacts are ready. You can find them in:\n{}\n\n",
            size_diff,
            build_mode,
            self.target_directory.display().to_string().bold(),
        );
        if let Some(metadata_result) = self.metadata_result.as_ref() {
            let bundle = format!(
                "  - {} (code + metadata)\n",
                util::base_name(&metadata_result.dest_bundle).bold()
            );
            out.push_str(&bundle);
        }
        if let Some(dest_wasm) = self.dest_wasm.as_ref() {
            let wasm = format!(
                "  - {} (the contract's code)\n",
                util::base_name(dest_wasm).bold()
            );
            out.push_str(&wasm);
        }
        if let Some(metadata_result) = self.metadata_result.as_ref() {
            let metadata = format!(
                "  - {} (the contract's metadata)",
                util::base_name(&metadata_result.dest_metadata).bold()
            );
            out.push_str(&metadata);
        }
        out
    }

    /// Returns a tuple of `(original_size, optimized_size)`.
    ///
    /// Panics if no optimization result is available.
    fn display_optimization(&self) -> (f64, f64) {
        let optimization = self
            .optimization_result
            .as_ref()
            .expect("optimization result must exist");
        (optimization.original_size, optimization.optimized_size)
    }

    /// Display the build results in a pretty formatted JSON string.
    pub fn serialize_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Setup and create a new smart contract project
    #[clap(name = "new")]
    New {
        /// The name of the newly created smart contract
        name: String,
        /// The optional target directory for the contract project
        #[clap(short, long, parse(from_os_str))]
        target_dir: Option<PathBuf>,
    },
    /// Compiles the contract, generates metadata, bundles both together in a `<name>.contract` file
    #[clap(name = "build")]
    Build(BuildCommand),
    /// Check that the code builds as Wasm; does not output any `<name>.contract` artifact to the `target/` directory
    #[clap(name = "check")]
    Check(CheckCommand),
    /// Test the smart contract off-chain
    #[clap(name = "test")]
    Test(TestCommand),
    /// Upload contract code
    #[clap(name = "upload")]
    Upload(UploadCommand),
    /// Instantiate a contract
    #[clap(name = "instantiate")]
    Instantiate(InstantiateCommand),
    /// Call a contract
    #[clap(name = "call")]
    Call(CallCommand),
    /// Decodes a contracts input or output data (supplied in hex-encoding)
    #[clap(name = "decode")]
    Decode(DecodeCommand),
}

fn main() {
    tracing_subscriber::fmt::init();

    let Opts::Contract(args) = Opts::parse();
    match exec(args.cmd) {
        Ok(()) => {}
        Err(err) => {
            eprintln!(
                "{} {}",
                "ERROR:".bright_red().bold(),
                format!("{:?}", err).bright_red()
            );
            std::process::exit(1);
        }
    }
}

fn exec(cmd: Command) -> Result<()> {
    match &cmd {
        Command::New { name, target_dir } => {
            cmd::new::execute(name, target_dir.as_ref())?;
            println!("Created contract {}", name);
            Ok(())
        }
        Command::Build(build) => {
            let results = build.exec()?;

            for (i, result) in results.iter().enumerate() {
                if matches!(result.output_type, OutputType::Json) {
                    println!("{}", result.serialize_json()?)
                } else if result.verbosity.is_verbose() {
                    if results.len() > 1 {
                        println!(
                            "\n{} [{}/{}]",
                            "Results for contract".bright_cyan().bold(),
                            i + 1,
                            results.len()
                        );
                    }
                    println!("{}", result.display())
                }
            }

            Ok(())
        }
        Command::Check(check) => {
            let results = check.exec()?;
            for res in results {
                assert!(
                    res.dest_wasm.is_none(),
                    "no dest_wasm must be on the generation result"
                );
            }
            Ok(())
        }
        Command::Test(test) => {
            let results = test.exec()?;
            for res in results {
                if res.verbosity.is_verbose() {
                    println!("{}", res.display()?)
                }
            }
            Ok(())
        }
        Command::Upload(upload) => upload.run(),
        Command::Instantiate(instantiate) => instantiate.run(),
        Command::Call(call) => call.run(),
        Command::Decode(decode) => decode.run(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_result_seralization_sanity_check() {
        // given
        let raw_result = r#"{
  "dest_wasm": "/path/to/contract.wasm",
  "metadata_result": {
    "dest_metadata": "/path/to/metadata.json",
    "dest_bundle": "/path/to/contract.contract"
  },
  "target_directory": "/path/to/target",
  "optimization_result": {
    "dest_wasm": "/path/to/contract.wasm",
    "original_size": 64.0,
    "optimized_size": 32.0
  },
  "build_mode": "Debug",
  "build_artifact": "All",
  "verbosity": "Quiet"
}"#;

        let build_result = BuildResult {
            dest_wasm: Some(PathBuf::from("/path/to/contract.wasm")),
            metadata_result: Some(MetadataResult {
                dest_metadata: PathBuf::from("/path/to/metadata.json"),
                dest_bundle: PathBuf::from("/path/to/contract.contract"),
            }),
            target_directory: PathBuf::from("/path/to/target"),
            optimization_result: Some(OptimizationResult {
                dest_wasm: PathBuf::from("/path/to/contract.wasm"),
                original_size: 64.0,
                optimized_size: 32.0,
            }),
            build_mode: Default::default(),
            build_artifact: Default::default(),
            verbosity: Verbosity::Quiet,
            output_type: OutputType::Json,
        };

        // when
        let serialized_result = build_result.serialize_json();

        // then
        assert!(serialized_result.is_ok());
        assert_eq!(serialized_result.unwrap(), raw_result);
    }
}

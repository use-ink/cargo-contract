// Copyright 2018-2021 Parity Technologies (UK) Ltd.
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

mod cmd;
mod crate_metadata;
mod util;
mod validate_wasm;
mod workspace;

use self::workspace::ManifestPath;

use crate::cmd::{metadata::MetadataResult, BuildCommand, CheckCommand, TestCommand};

#[cfg(feature = "extrinsics")]
use sp_core::{crypto::Pair, sr25519, H256};
use std::{
    convert::TryFrom,
    fmt::{Display, Formatter, Result as DisplayResult},
    path::PathBuf,
    str::FromStr,
};
#[cfg(feature = "extrinsics")]
use subxt::PairSigner;

use anyhow::{Error, Result};
use colored::Colorize;
use structopt::{clap, StructOpt};

#[derive(Debug, StructOpt)]
#[structopt(bin_name = "cargo")]
#[structopt(version = env!("CARGO_CONTRACT_CLI_IMPL_VERSION"))]
pub(crate) enum Opts {
    /// Utilities to develop Wasm smart contracts.
    #[structopt(name = "contract")]
    #[structopt(version = env!("CARGO_CONTRACT_CLI_IMPL_VERSION"))]
    #[structopt(setting = clap::AppSettings::UnifiedHelpMessage)]
    #[structopt(setting = clap::AppSettings::DeriveDisplayOrder)]
    #[structopt(setting = clap::AppSettings::DontCollapseArgsInUsage)]
    Contract(ContractArgs),
}

#[derive(Debug, StructOpt)]
pub(crate) struct ContractArgs {
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct HexData(pub Vec<u8>);

#[cfg(feature = "extrinsics")]
impl std::str::FromStr for HexData {
    type Err = hex::FromHexError;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        hex::decode(input).map(HexData)
    }
}

/// Arguments required for creating and sending an extrinsic to a substrate node
#[cfg(feature = "extrinsics")]
#[derive(Debug, StructOpt)]
pub(crate) struct ExtrinsicOpts {
    /// Websockets url of a substrate node
    #[structopt(
        name = "url",
        long,
        parse(try_from_str),
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
    /// Secret key URI for the account deploying the contract.
    #[structopt(name = "suri", long, short)]
    suri: String,
    /// Password for the secret key
    #[structopt(name = "password", long, short)]
    password: Option<String>,
}

#[cfg(feature = "extrinsics")]
impl ExtrinsicOpts {
    pub fn signer(&self) -> Result<PairSigner<subxt::DefaultNodeRuntime, sr25519::Pair>> {
        let pair =
            sr25519::Pair::from_string(&self.suri, self.password.as_ref().map(String::as_ref))
                .map_err(|_| anyhow::anyhow!("Secret string error"))?;
        Ok(PairSigner::new(pair))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
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

impl std::str::FromStr for OptimizationPasses {
    type Err = Error;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        // We need to replace " here, since the input string could come
        // from either the CLI or the `Cargo.toml` profile section.
        // If it is from the profile it could e.g. be "3" or 3.
        let normalized_input = input.replace("\"", "").to_lowercase();
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

impl From<std::string::String> for OptimizationPasses {
    fn from(str: String) -> Self {
        OptimizationPasses::from_str(&str).expect("conversion failed")
    }
}

#[derive(Default, Clone, Debug, StructOpt)]
pub struct VerbosityFlags {
    /// No output printed to stdout
    #[structopt(long)]
    quiet: bool,
    /// Use verbose output
    #[structopt(long)]
    verbose: bool,
}

/// Denotes if output should be printed to stdout.
#[derive(Clone, Copy, serde::Serialize)]
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

#[derive(Default, Clone, Debug, StructOpt)]
struct UnstableOptions {
    /// Use the original manifest (Cargo.toml), do not modify for build optimizations
    #[structopt(long = "unstable-options", short = "Z", number_of_values = 1)]
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
#[derive(Copy, Clone, Eq, PartialEq, Debug, StructOpt, serde::Serialize)]
#[structopt(name = "build-artifacts")]
pub enum BuildArtifacts {
    /// Generate the Wasm, the metadata and a bundled `<name>.contract` file
    #[structopt(name = "all")]
    All,
    /// Only the Wasm is created, generation of metadata and a bundled `<name>.contract` file is skipped
    #[structopt(name = "code-only")]
    CodeOnly,
    CheckOnly,
}

impl BuildArtifacts {
    /// Returns the number of steps required to complete a build artifact.
    /// Used as output on the cli.
    pub fn steps(&self) -> usize {
        match self {
            BuildArtifacts::All => 5,
            BuildArtifacts::CodeOnly => 3,
            BuildArtifacts::CheckOnly => 2,
        }
    }
}

impl std::str::FromStr for BuildArtifacts {
    type Err = String;
    fn from_str(artifact: &str) -> Result<Self, Self::Err> {
        match artifact {
            "all" => Ok(BuildArtifacts::All),
            "code-only" => Ok(BuildArtifacts::CodeOnly),
            _ => Err("Could not parse build artifact".to_string()),
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
    /// The type of formatting to use for the built output.
    #[serde(skip_serializing)]
    pub output_type: OutputType,
}

/// Result of the optimization process.
#[derive(serde::Serialize)]
pub struct OptimizationResult {
    /// The path of the optimized wasm file.
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
            return out;
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

#[derive(Debug, StructOpt)]
enum Command {
    /// Setup and create a new smart contract project
    #[structopt(name = "new")]
    New {
        /// The name of the newly created smart contract
        name: String,
        /// The optional target directory for the contract project
        #[structopt(short, long, parse(from_os_str))]
        target_dir: Option<PathBuf>,
    },
    /// Compiles the contract, generates metadata, bundles both together in a `<name>.contract` file
    #[structopt(name = "build")]
    Build(BuildCommand),
    /// Check that the code builds as Wasm; does not output any `<name>.contract` artifact to the `target/` directory
    #[structopt(name = "check")]
    Check(CheckCommand),
    /// Test the smart contract off-chain
    #[structopt(name = "test")]
    Test(TestCommand),
    /// Upload the smart contract code to the chain
    #[cfg(feature = "extrinsics")]
    #[structopt(name = "deploy")]
    Deploy {
        #[structopt(flatten)]
        extrinsic_opts: ExtrinsicOpts,
        /// Path to wasm contract code, defaults to `./target/ink/<name>.wasm`
        #[structopt(parse(from_os_str))]
        wasm_path: Option<PathBuf>,
    },
    /// Instantiate a deployed smart contract
    #[cfg(feature = "extrinsics")]
    #[structopt(name = "instantiate")]
    Instantiate {
        #[structopt(flatten)]
        extrinsic_opts: ExtrinsicOpts,
        /// Transfers an initial balance to the instantiated contract
        #[structopt(name = "endowment", long, default_value = "0")]
        endowment: u128,
        /// Maximum amount of gas to be used for this command
        #[structopt(name = "gas", long, default_value = "500000000")]
        gas_limit: u64,
        /// The hash of the smart contract code already uploaded to the chain
        #[structopt(long, parse(try_from_str = parse_code_hash))]
        code_hash: H256,
        /// Hex encoded data to call a contract constructor
        #[structopt(long)]
        data: HexData,
    },
}

#[cfg(feature = "extrinsics")]
fn parse_code_hash(input: &str) -> Result<H256> {
    let bytes = hex::decode(input)?;
    if bytes.len() != 32 {
        anyhow::bail!("Code hash should be 32 bytes in length")
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(H256(arr))
}

fn main() {
    env_logger::init();

    let Opts::Contract(args) = Opts::from_args();
    match exec(args.cmd) {
        Ok(maybe_msg) => {
            if let Some(msg) = maybe_msg {
                println!("\t{}", msg)
            }
        }
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

fn exec(cmd: Command) -> Result<Option<String>> {
    match &cmd {
        Command::New { name, target_dir } => cmd::new::execute(name, target_dir.as_ref()),
        Command::Build(build) => {
            let result = build.exec()?;

            if matches!(result.output_type, OutputType::Json) {
                Ok(Some(result.serialize_json()?))
            } else if result.verbosity.is_verbose() {
                Ok(Some(result.display()))
            } else {
                Ok(None)
            }
        }
        Command::Check(check) => {
            let res = check.exec()?;
            assert!(
                res.dest_wasm.is_none(),
                "no dest_wasm must be on the generation result"
            );
            if res.verbosity.is_verbose() {
                Ok(Some(
                    "\nYour contract's code was built successfully.".to_string(),
                ))
            } else {
                Ok(None)
            }
        }
        Command::Test(test) => {
            let res = test.exec()?;
            if res.verbosity.is_verbose() {
                Ok(Some(res.display()?))
            } else {
                Ok(None)
            }
        }
        #[cfg(feature = "extrinsics")]
        Command::Deploy {
            extrinsic_opts,
            wasm_path,
        } => {
            let code_hash = cmd::execute_deploy(extrinsic_opts, wasm_path.as_ref())?;
            Ok(Some(format!("Code hash: {:?}", code_hash)))
        }
        #[cfg(feature = "extrinsics")]
        Command::Instantiate {
            extrinsic_opts,
            endowment,
            code_hash,
            gas_limit,
            data,
        } => {
            let contract_account = cmd::execute_instantiate(
                extrinsic_opts,
                *endowment,
                *gas_limit,
                *code_hash,
                data.clone(),
            )?;
            Ok(Some(format!("Contract account: {:?}", contract_account)))
        }
    }
}

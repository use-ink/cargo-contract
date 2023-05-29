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
use contract_build::{
    BuildArtifacts,
    BuildMode,
    BuildResult,
    ExecuteArgs,
    Features,
    ManifestPath,
    MetadataArtifacts,
    Network,
    OptimizationPasses,
    OutputType,
    Target,
    UnstableFlags,
    UnstableOptions,
    Verbosity,
    VerbosityFlags,
};
use byte_unit::Byte;
use serde_json::{
    Map,
    Value,
};
use std::{
    collections::HashMap,
    convert::TryFrom,
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
};

/// Executes build of the smart contract which produces a Wasm binary that is ready for
/// deploying.
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
    /// Performs linting checks during the build process
    #[clap(long)]
    lint: bool,
    /// Which build artifacts to generate.
    ///
    /// - `all`: Generate the Wasm, the metadata and a bundled `<name>.contract` file.
    ///
    /// - `code-only`: Only the Wasm is created, generation of metadata and a bundled
    ///   `<name>.contract` file is skipped.
    ///
    /// - `check-only`: No artifacts produced: runs the `cargo check` command for the
    ///   Wasm target, only checks for compilation errors.
    #[clap(long = "generate", value_enum, default_value = "all")]
    build_artifact: BuildArtifacts,
    #[clap(flatten)]
    features: Features,
    #[clap(flatten)]
    verbosity: VerbosityFlags,
    #[clap(flatten)]
    unstable_options: UnstableOptions,
    /// Number of optimization passes, passed as an argument to `wasm-opt`.
    ///
    /// - `0`: execute no optimization passes
    ///
    /// - `1`: execute 1 optimization pass (quick & useful opts, useful for iteration
    ///   builds)
    ///
    /// - `2`, execute 2 optimization passes (most opts, generally gets most perf)
    ///
    /// - `3`, execute 3 optimization passes (spends potentially a lot of time
    ///   optimizing)
    ///
    /// - `4`, execute 4 optimization passes (also flatten the IR, which can take a lot
    ///   more time and memory but is useful on more nested / complex / less-optimized
    ///   input)
    ///
    /// - `s`, execute default optimization passes, focusing on code size
    ///
    /// - `z`, execute default optimization passes, super-focusing on code size
    ///
    /// - The default value is `z`
    ///
    /// - It is possible to define the number of optimization passes in the
    ///   `[package.metadata.contract]` of your `Cargo.toml` as e.g. `optimization-passes
    ///   = "3"`. The CLI argument always takes precedence over the profile value.
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
    /// Don't perform wasm validation checks e.g. for permitted imports.
    #[clap(long)]
    skip_wasm_validation: bool,
    /// Which bytecode to build the contract into.
    #[clap(long, default_value = "wasm")]
    target: Target,
    /// The maximum number of pages available for a wasm contract to allocate.
    #[clap(long, default_value_t = contract_build::DEFAULT_MAX_MEMORY_PAGES)]
    max_memory_pages: u32,
}

impl BuildCommand {
    pub fn exec(&self) -> Result<BuildResult> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let unstable_flags: UnstableFlags =
            TryFrom::<&UnstableOptions>::try_from(&self.unstable_options)?;
        let mut verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;

        let build_mode = match self.build_release {
            true => BuildMode::Release,
            false => BuildMode::Debug,
        };

        let network = match self.build_offline {
            true => Network::Offline,
            false => Network::Online,
        };

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
            features: self.features.clone(),
            network,
            build_artifact: self.build_artifact,
            unstable_flags,
            optimization_passes: self.optimization_passes,
            keep_debug_symbols: self.keep_debug_symbols,
            lint: self.lint,
            output_type,
            skip_wasm_validation: self.skip_wasm_validation,
            target: self.target,
            max_memory_pages: self.max_memory_pages,
        };

        let build_result: BuildResult = match contract_build::execute(args) {
            Ok(b) => b,
            _ => anyhow::bail!("Unable to execute build of the smart contract"),
        };
        println!("build_result {:#?}", build_result);
        let mut target_directory = build_result.target_directory
            .as_path().display().to_string();
        let target_directory_short = match &target_directory.rfind("target") {
            Some(index) => target_directory.split_off(*index),
            None => "".to_string(), // unknown target directory
        };
        println!("target_directory_short: {}", &target_directory_short);

        let metadata_artifacts: &MetadataArtifacts =
            match &build_result.metadata_result {
                Some(ma) => ma,
                None => anyhow::bail!("Missing metadata_result in build result"),
            };
        let metadata_json_path = metadata_artifacts.dest_metadata
            .as_path().display().to_string();
        println!("metadata_json_path {:?}", metadata_json_path);
        let file_metadata = File::open(metadata_json_path)?;
        let mut buf_reader = BufReader::new(&file_metadata);
        let mut contents = String::new();
        buf_reader.read_to_string(&mut contents)?;
        let file_metadata_len = &file_metadata.metadata().unwrap().len();
        let byte = Byte::from_bytes(<u64 as Into<u128>>::into(*file_metadata_len));
        let adjusted_byte = byte.get_appropriate_unit(false);
        let file_len_units = &adjusted_byte.to_string();
        println!("file len in units {}", &adjusted_byte.to_string());

        let metadata_json: Map<String, Value> =
            serde_json::from_slice::<Map<String, Value>>(&contents.as_bytes())?;
        let contract_name = metadata_json["storage"]["root"]["layout"]["struct"]["name"].as_str().unwrap();
        println!("contract_name {:?}", &contract_name);
        // println!("metadata_json {:?}", metadata_json);
        let contract_map = HashMap::from([
            ("Contract", contract_name),
            ("Size", file_len_units),
            ("Metadata Path", &target_directory_short),
        ]);
        let build_data = vec![
            &contract_map
        ];
        println!("contract_map {:#?}", contract_map.clone());
        println!("build_data {:#?}", &build_data);

        let build_info_path = "build_info.json";
        let exists_build_info_path = std::path::Path::new(build_info_path).exists();
        if !exists_build_info_path {
            println!("existing path");
            // build_info.json doesn't exist, so create it with the data
            serde_json::to_writer(&File::create("build_info.json")?, &build_data)?;
        } else {
            println!("not existing path");
            // build_info.json exists, so update it with the data
            let file_build_info = File::open(build_info_path)?;
            buf_reader = BufReader::new(&file_build_info);
            contents = String::new();
            buf_reader.read_to_string(&mut contents)?;
            let build_info_json: Vec<HashMap<&str, &str>> =
                serde_json::from_slice::<Vec<HashMap<&str, &str>>>(&contents.as_bytes())?;
            println!("build_info_json {:#?}", build_info_json);

            let mut _new_build_data: Vec<HashMap<&str, &str>> = vec![];
            let mut _serialized_data: &str;
            let mut _info_hashmap: &HashMap<&str, &str>;

            for info in build_info_json.iter() {
                // serialized_data = serde_json::to_string(&info).unwrap().as_str();
                let serialized_data_owned: &str = &serde_json::to_string(&info).unwrap();
                _serialized_data = &serialized_data_owned;
                let info_hashmap_owned = serde_json::from_str(&serialized_data_owned).unwrap();
                _info_hashmap = &info_hashmap_owned;

                // println!("{:#?}", info);
                for (label, val) in info.clone() {
                    println!("{label:?} has {val}");
                    // replace existing build info with new contract info
                    // if the contract name already exists as a value in build_info.json
                    if val == contract_name {
                        &_new_build_data.push(contract_map.clone());
                    // otherwise keep the existing contract info object
                    } else {
                        &_new_build_data.push(info_hashmap_owned.clone());
                    }
                }
            }
            // write updated to file
            serde_json::to_writer(&File::create("build_info.json")?,
                &_new_build_data.clone())?;
        }

        Ok(build_result)
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
    features: Features,
    #[clap(flatten)]
    unstable_options: UnstableOptions,
    #[clap(long, default_value = "wasm")]
    target: Target,
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
            features: self.features.clone(),
            network: Network::default(),
            build_artifact: BuildArtifacts::CheckOnly,
            unstable_flags,
            optimization_passes: Some(OptimizationPasses::Zero),
            keep_debug_symbols: false,
            lint: false,
            output_type: OutputType::default(),
            skip_wasm_validation: false,
            target: self.target,
            max_memory_pages: 0,
        };

        contract_build::execute(args)
    }
}

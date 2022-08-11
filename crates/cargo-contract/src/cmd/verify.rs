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
    cmd::{
        build::{
            execute,
            ExecuteArgs,
        },
        metadata::BuildInfo,
    },
    workspace::ManifestPath,
    BuildArtifacts,
};

use anyhow::Result;
use contract_metadata::ContractMetadata;

use std::{
    fs::File,
    io::prelude::Read,
    path::PathBuf,
};

#[derive(Debug, clap::Args)]
#[clap(name = "verify")]
pub struct VerifyCommand {
    /// Path to the `Cargo.toml` of the contract to verify.
    #[clap(long, parse(from_os_str))]
    manifest_path: Option<PathBuf>,
    /// The reference Wasm contract (`*.contract`) that the workspace will be checked against.
    contract: PathBuf,
}

impl VerifyCommand {
    pub fn run(&self) -> Result<()> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;

        // 1. Read the given metadata, and pull out the `BuildInfo`
        let mut file = File::open(&self.contract)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let metadata: ContractMetadata = serde_json::from_str(&contents)?;
        let build_info = metadata.source.build_info.as_ref().unwrap();
        let build_info: BuildInfo =
            serde_json::from_value(build_info.clone().into()).unwrap();

        // 2. Call `cmd::Build` with the given `BuildInfo`
        let args = ExecuteArgs {
            manifest_path: manifest_path.clone(),
            verbosity: Default::default(),
            build_mode: build_info.build_mode,
            network: Default::default(),
            build_artifact: BuildArtifacts::CodeOnly,
            unstable_flags: Default::default(),
            optimization_passes: build_info.wasm_opt_settings.optimization_passes,
            keep_debug_symbols: false, /* TODO: Will either want to add this to BuildInfo or assume release (so no) */
            skip_linting: true,
            output_type: Default::default(),
        };

        let build_result = execute(args)?;

        // 3. Read output file, compare with given contract_wasm
        let reference_wasm = metadata.source.wasm.unwrap().to_string();

        let built_wasm_path = build_result.dest_wasm.unwrap();
        let fs_wasm = std::fs::read(built_wasm_path)?;
        let built_wasm = build_byte_str(&fs_wasm);

        if reference_wasm != built_wasm {
            log::debug!(
                "Expected Wasm Binary '{}'\n\nGot Wasm Binary `{}`",
                &reference_wasm,
                &built_wasm
            );
            anyhow::bail!(
                "Failed to verify the authenticity of `{}` contract againt the workspace found at {:?}.",
                metadata.contract.name,
                manifest_path.as_ref(),
            );
        }

        log::info!("Succesfully verified `{}`!", &metadata.contract.name);

        Ok(())
    }
}

fn build_byte_str(bytes: &[u8]) -> String {
    use std::fmt::Write;

    let mut str = String::new();
    write!(str, "0x").expect("failed writing to string");
    for byte in bytes {
        write!(str, "{:02x}", byte).expect("failed writing to string");
    }
    str
}

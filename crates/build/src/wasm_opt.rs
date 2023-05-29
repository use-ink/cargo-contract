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
use wasm_opt::OptimizationOptions;

use std::{
    fmt,
    path::PathBuf,
    str,
};

/// A helpful struct for interacting with Binaryen's `wasm-opt` tool.
pub struct WasmOptHandler {
    /// The optimization level that should be used when optimizing the Wasm binary.
    optimization_level: OptimizationPasses,
    /// Whether or not to keep debugging information in the final Wasm binary.
    keep_debug_symbols: bool,
}

impl WasmOptHandler {
    /// Generate a new instance of the handler.
    ///
    /// Fails if the `wasm-opt` binary is not installed on the system, or if an outdated
    /// `wasm-opt` binary is used (currently a version >= 99 is required).
    pub fn new(
        optimization_level: OptimizationPasses,
        keep_debug_symbols: bool,
    ) -> Result<Self> {
        Ok(Self {
            optimization_level,
            keep_debug_symbols,
        })
    }

    /// Attempts to perform optional Wasm optimization using Binaryen's `wasm-opt` tool.
    ///
    /// If successful, the optimized Wasm binary is written to `dest_wasm`.
    pub fn optimize(
        &self,
        dest_wasm: &PathBuf,
        contract_artifact_name: &String,
    ) -> Result<()> {
        // We'll create a temporary file for our optimized Wasm binary. Note that we'll
        // later overwrite this with the original path of the Wasm binary.
        let mut dest_optimized = dest_wasm.clone();
        dest_optimized.set_file_name(format!("{contract_artifact_name}-opt.wasm"));

        tracing::debug!(
            "Optimization level passed to wasm-opt: {}",
            self.optimization_level
        );

        OptimizationOptions::from(self.optimization_level)
            // Binaryen (and wasm-opt) now enables the `SignExt` and `MutableGlobals`
            // features by default, so we want to disable those for now since
            // `pallet-contracts` still needs to enable these.
            .mvp_features_only()
            // the memory in our module is imported, `wasm-opt` needs to be told that
            // the memory is initialized to zeroes, otherwise it won't run the
            // memory-packing pre-pass.
            .zero_filled_memory(true)
            .debug_info(self.keep_debug_symbols)
            .run(dest_wasm, &dest_optimized)?;

        if !dest_optimized.exists() {
            return Err(anyhow::anyhow!(
                "Optimization failed, optimized wasm output file `{}` not found.",
                dest_optimized.display()
            ))
        }

        // Overwrite existing destination wasm file with the optimised version
        std::fs::rename(&dest_optimized, dest_wasm)?;
        Ok(())
    }
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize,
)]
pub enum OptimizationPasses {
    Zero,
    One,
    Two,
    Three,
    Four,
    S,
    #[default]
    Z,
}

impl fmt::Display for OptimizationPasses {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let out = match self {
            OptimizationPasses::Zero => "0",
            OptimizationPasses::One => "1",
            OptimizationPasses::Two => "2",
            OptimizationPasses::Three => "3",
            OptimizationPasses::Four => "4",
            OptimizationPasses::S => "s",
            OptimizationPasses::Z => "z",
        };
        write!(f, "{out}")
    }
}

impl str::FromStr for OptimizationPasses {
    type Err = anyhow::Error;

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
        <OptimizationPasses as str::FromStr>::from_str(&str).expect("conversion failed")
    }
}

impl From<OptimizationPasses> for OptimizationOptions {
    fn from(passes: OptimizationPasses) -> OptimizationOptions {
        match passes {
            OptimizationPasses::Zero => OptimizationOptions::new_opt_level_0(),
            OptimizationPasses::One => OptimizationOptions::new_opt_level_1(),
            OptimizationPasses::Two => OptimizationOptions::new_opt_level_2(),
            OptimizationPasses::Three => OptimizationOptions::new_opt_level_3(),
            OptimizationPasses::Four => OptimizationOptions::new_opt_level_4(),
            OptimizationPasses::S => OptimizationOptions::new_optimize_for_size(),
            OptimizationPasses::Z => {
                OptimizationOptions::new_optimize_for_size_aggressively()
            }
        }
    }
}

/// Result of the optimization process.
#[derive(Debug, serde::Serialize)]
pub struct OptimizationResult {
    /// The path of the optimized Wasm file.
    pub dest_wasm: PathBuf,
    /// The original Wasm size.
    pub original_size: f64,
    /// The Wasm size after optimizations have been applied.
    pub optimized_size: f64,
}

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
    OptimizationPasses,
    OptimizationResult,
};

use anyhow::Result;
use wasm_opt::OptimizationOptions;

use std::{
    fs::metadata,
    path::PathBuf,
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
    /// Fails if the `wasm-opt` binary is not installed on the system, or if an outdated `wasm-opt`
    /// binary is used (currently a version >= 99 is required).
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
    ) -> Result<OptimizationResult> {
        // We'll create a temporary file for our optimized Wasm binary. Note that we'll later
        // overwrite this with the original path of the Wasm binary.
        let mut dest_optimized = dest_wasm.clone();
        dest_optimized.set_file_name(format!("{}-opt.wasm", contract_artifact_name));

        tracing::debug!(
            "Optimization level passed to wasm-opt: {}",
            self.optimization_level
        );

        OptimizationOptions::from(self.optimization_level)
            // the memory in our module is imported, `wasm-opt` needs to be told that
            // the memory is initialized to zeroes, otherwise it won't run the
            // memory-packing pre-pass.
            .zero_filled_memory(true)
            .debug_info(self.keep_debug_symbols)
            .run(&dest_wasm, &dest_optimized)?;

        if !dest_optimized.exists() {
            return Err(anyhow::anyhow!(
                "Optimization failed, optimized wasm output file `{}` not found.",
                dest_optimized.display()
            ))
        }

        let original_size = metadata(&dest_wasm)?.len() as f64 / 1000.0;
        let optimized_size = metadata(&dest_optimized)?.len() as f64 / 1000.0;

        // Overwrite existing destination wasm file with the optimised version
        std::fs::rename(&dest_optimized, &dest_wasm)?;
        Ok(OptimizationResult {
            dest_wasm: dest_wasm.clone(),
            original_size,
            optimized_size,
        })
    }
}

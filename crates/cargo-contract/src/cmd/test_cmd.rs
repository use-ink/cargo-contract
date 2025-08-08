// Copyright (C) ink! contributors.
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

use std::path::PathBuf;

use contract_build::{
    util,
    CrateMetadata,
    Features,
    ManifestPath,
    VerbosityFlags,
};

/// Execute all unit and integration tests and build examples.
#[derive(Debug, clap::Args)]
#[clap(name = "test")]
pub struct TestCommand {
    /// Path to the `Cargo.toml` of the contract to test.
    #[clap(long, value_parser)]
    manifest_path: Option<PathBuf>,
    #[clap(flatten)]
    features: Features,
    /// Activate all available features.
    #[clap(long)]
    all_features: bool,
    #[clap(flatten)]
    verbosity: VerbosityFlags,
    /// Disable capturing of test output (e.g. for debugging).
    #[clap(long)]
    nocapture: bool,
    /// Arguments to pass to `cargo test`.
    #[arg(last = true)]
    args: Vec<String>,
}

impl TestCommand {
    pub fn run(&self) -> anyhow::Result<()> {
        // Composes `cargo test` args.
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let mut args = vec![manifest_path.cargo_arg()?];
        self.features.append_to_args(&mut args);
        if self.all_features {
            args.push("--all-features".to_string());
        }
        if !self.args.is_empty() {
            args.extend(self.args.clone());
        }
        if self.nocapture {
            // Adds escape arg (if necessary).
            if !self.args.iter().any(|arg| arg == "--") {
                args.push("--".to_string());
            }
            args.push("--nocapture".to_string());
        }

        // Composes ABI `cfg` flag.
        let mut env = Vec::new();
        let crate_metadata = CrateMetadata::collect(&manifest_path)?;
        if let Some(abi) = crate_metadata.abi {
            env.push((
                "CARGO_ENCODED_RUSTFLAGS",
                Some(abi.cargo_encoded_rustflag()),
            ));
        }

        // Runs `cargo test`.
        let verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;
        let cmd = util::cargo_cmd(
            "test",
            args,
            crate_metadata.manifest_path.directory(),
            verbosity,
            env,
        );
        let output = cmd.run()?;
        if !output.status.success() {
            anyhow::bail!(
                "Failed to run `cargo test`{}",
                if output.stderr.is_empty() {
                    String::new()
                } else {
                    format!(": {}", String::from_utf8_lossy(&output.stderr))
                }
            )
        }

        Ok(())
    }
}

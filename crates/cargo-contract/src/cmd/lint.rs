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
    lint,
    CrateMetadata,
    ManifestPath,
    VerbosityFlags,
};
use std::path::PathBuf;

/// Lints a contract.
#[derive(Debug, clap::Args)]
#[clap(name = "lint")]
pub struct LintCommand {
    /// Path to the `Cargo.toml` of the contract to build
    #[clap(long, value_parser)]
    manifest_path: Option<PathBuf>,
    #[clap(flatten)]
    verbosity: VerbosityFlags,
    /// Performs extra linting checks during the build process. Basic clippy
    /// lints are deemed important and run anyway.
    #[clap(long)]
    extra_lints: bool,
}

impl LintCommand {
    pub fn run(&self) -> Result<()> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let crate_metadata = CrateMetadata::collect(&manifest_path)?;
        let verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;
        lint(self.extra_lints, &crate_metadata, &verbosity)
    }
}

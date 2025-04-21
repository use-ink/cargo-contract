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
    /// Performs extra linting checks for ink! specific issues during the
    /// build process.
    ///
    /// Basic clippy lints are deemed important and run anyway.
    #[clap(long)]
    lint: bool,
}

impl LintCommand {
    pub fn run(&self) -> Result<()> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let crate_metadata = CrateMetadata::collect(&manifest_path)?;
        let verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;
        lint(self.lint, &crate_metadata, &verbosity)
    }
}

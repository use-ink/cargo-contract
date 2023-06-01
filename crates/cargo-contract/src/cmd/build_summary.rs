use crate::cmd::printer::print_build_info;
use anyhow::{Error, Result};

#[derive(Debug, clap::Args)]
#[clap(name = "summary", about = "Get info about built contracts")]
pub struct SummaryCommand {
    /// Export the built contracts info output in JSON format.
    #[clap(name = "output-build-info-json", long)]
    build_info_json: bool,
}

impl SummaryCommand {
    pub fn run(&self) -> Result<(), Error> {
        print_build_info(false, self.build_info_json, None, None)?;

        Ok(())
    }
}

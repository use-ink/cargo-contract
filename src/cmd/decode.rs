// Copyright 2018-2020 Parity Technologies (UK) Ltd.
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
    cmd::extrinsics::{load_metadata, ContractMessageTranscoder},
    util::decode_hex,
    DEFAULT_KEY_COL_WIDTH,
};
use anyhow::{Context, Result};
use colored::Colorize as _;

#[derive(Debug, Clone, clap::Args)]
#[clap(name = "decode", about = "Decode input_data for a contract")]
pub struct DecodeCommand {
    /// Type of data
    #[clap(arg_enum, short, long)]
    r#type: DataType,
    /// The data to decode
    #[clap(long)]
    data: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ArgEnum)]
enum DataType {
    Event,
    Message,
}

impl DecodeCommand {
    pub fn run(&self) -> Result<()> {
        let (_, contract_metadata) = load_metadata(None)?;
        let transcoder = ContractMessageTranscoder::new(&contract_metadata);

        const ERR_MSG: &str = "Failed to decode specified data as a hex value";
        let decoded_data = match self.r#type {
            DataType::Event => {
                transcoder.decode_contract_event(&mut &decode_hex(&self.data).context(ERR_MSG)?[..])
            }
            DataType::Message => transcoder
                .decode_contract_message(&mut &decode_hex(&self.data).context(ERR_MSG)?[..]),
        };

        println!(
            "{:>width$} {}",
            "Decoded data:".bright_green().bold(),
            decoded_data,
            width = DEFAULT_KEY_COL_WIDTH
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::util::tests::with_new_contract_project;
    use std::path::Path;

    /// Create a `cargo contract` command
    fn cargo_contract(path: &Path) -> assert_cmd::Command {
        let mut cmd = assert_cmd::Command::cargo_bin("cargo-contract").unwrap();
        cmd.current_dir(path).arg("contract");
        cmd
    }

    #[test]
    fn decode_works() {
        // given
        let contract = r#"
	#![cfg_attr(not(feature = "std"), no_std)]

	use ink_lang as ink;

	#[ink::contract]
	pub mod switcher {
	    #[ink(event)]
	    pub struct Switched {
		new_value: bool,
	    }

	    #[ink(storage)]
	    pub struct Switcher {
		value: bool,
	    }

	    impl Switcher {
		#[ink(constructor)]
		pub fn new(init_value: bool) -> Self {
		    Self { value: init_value }
		}

		#[ink(message, selector = 0xBABEBABE)]
		pub fn switch(&mut self, value: bool) {
		    self.value = value;
		}
	    }
	}"#;

        // when
        // contract is built
        with_new_contract_project(|manifest_path| {
            let project_dir = manifest_path.directory().expect("directory must exist");
            let lib = project_dir.join("lib.rs");
            std::fs::write(&lib, contract)?;

            assert_cmd::Command::new("rustup")
                .arg("override")
                .arg("set")
                .arg("nightly")
                .assert()
                .success();

            log::info!("Building contract in {}", project_dir.to_string_lossy());
            cargo_contract(project_dir).arg("build").assert().success();

            let msg_data: &str = "babebabe01";
            let msg_decoded: &str =
                r#"Ok(Map(Map { ident: Some("switch"), map: {String("value"): Bool(true)} }))"#;

            // then
            // message data is being decoded properly
            cargo_contract(project_dir)
                .arg("decode")
                .arg("--data")
                .arg(msg_data)
                .arg("-t")
                .arg("message")
                .assert()
                .success()
                .stdout(predicates::str::contains(msg_decoded));

            let event_data: &str = "080001";
            let event_decoded: &str = r#"Ok(Map(Map { ident: Some("Switched"), map: {String("new_value"): Bool(true)} }))"#;

            // and
            // event data is being decoded properly
            cargo_contract(project_dir)
                .arg("decode")
                .arg("--data")
                .arg(event_data)
                .arg("-t")
                .arg("event")
                .assert()
                .success()
                .stdout(predicates::str::contains(event_decoded));

            Ok(())
        })
    }
}

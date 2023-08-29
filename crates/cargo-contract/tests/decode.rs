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

use std::path::Path;

/// Create a `cargo contract` command
fn cargo_contract<P: AsRef<Path>>(path: P) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    cmd.current_dir(path).arg("contract");
    cmd
}

#[test]
fn decode_works() {
    // given
    let contract = r#"
        #![cfg_attr(not(feature = "std"), no_std, no_main)]

		#[ink::contract]
		mod switcher {
			#[ink(event)]
			pub struct Switched {
				new_value: bool,
			}

			#[ink(storage)]
			pub struct Switcher {
				value: bool,
			}

			impl Switcher {
				#[ink(constructor, selector = 0xBABEBABE)]
				pub fn new(init_value: bool) -> Self {
					Self { value: init_value }
				}

				#[ink(message, selector = 0xBABEBABE)]
				pub fn switch(&mut self, value: bool) {
					self.value = value;
				}
			}
		}"#;

    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");

    // cargo contract new decode_test
    cargo_contract(tmp_dir.path())
        .arg("new")
        .arg("switcher")
        .assert()
        .success();

    let project_dir = tmp_dir.path().to_path_buf().join("switcher");

    let lib = project_dir.join("lib.rs");
    std::fs::write(lib, contract).expect("Failed to write contract lib.rs");

    tracing::debug!("Building contract in {}", project_dir.to_string_lossy());
    cargo_contract(&project_dir).arg("build").assert().success();

    // when
    let msg_data: &str = "babebabe01";
    let msg_decoded: &str = r#"switch { value: true }"#;

    // then
    // message data is being decoded properly
    cargo_contract(&project_dir)
        .arg("decode")
        .arg("--data")
        .arg(msg_data)
        .arg("-t")
        .arg("message")
        .assert()
        .success()
        .stdout(predicates::str::contains(msg_decoded));

    // and when
    let wrong_msg_data: &str = "babebabe010A";
    let error_msg: &str = "input length was longer than expected by 1 byte(s).\nManaged to decode `switch`, `value` but `0A` bytes were left unread";

    // then
    // wrong message data is being handled properly
    cargo_contract(&project_dir)
        .arg("decode")
        .arg("--data")
        .arg(wrong_msg_data)
        .arg("-t")
        .arg("message")
        .assert()
        .failure()
        .stderr(predicates::str::contains(error_msg));

    // when
    let event_data: &str = "080001";
    let event_decoded: &str = r#"Switched { new_value: true }"#;

    // then
    // event data is being decoded properly
    cargo_contract(&project_dir)
        .arg("decode")
        .arg("--data")
        .arg(event_data)
        .arg("-t")
        .arg("event")
        .assert()
        .success()
        .stdout(predicates::str::contains(event_decoded));

    // and when
    let wrong_event_data: &str = "0800010C";
    let error_msg: &str = "input length was longer than expected by 1 byte(s).\nManaged to decode `Switched`, `new_value` but `0C` bytes were left unread";

    // then
    // wrong event data is being handled properly
    cargo_contract(&project_dir)
        .arg("decode")
        .arg("--data")
        .arg(wrong_event_data)
        .arg("-t")
        .arg("event")
        .assert()
        .failure()
        .stderr(predicates::str::contains(error_msg));

    // when
    let constructor_data: &str = "babebabe00";
    let constructor_decoded: &str = r#"new { init_value: false }"#;

    // then
    // constructor data is being decoded properly
    cargo_contract(&project_dir)
        .arg("decode")
        .arg("-d")
        .arg(constructor_data)
        .arg("-t")
        .arg("constructor")
        .assert()
        .success()
        .stdout(predicates::str::contains(constructor_decoded));

    // and when
    let wrong_constructor_data: &str = "babebabe00AC";
    let error_msg: &str = "input length was longer than expected by 1 byte(s).\nManaged to decode `new`, `init_value` but `AC` bytes were left unread";

    // then
    // wrong constructor data is being handled properly
    cargo_contract(&project_dir)
        .arg("decode")
        .arg("-d")
        .arg(wrong_constructor_data)
        .arg("-t")
        .arg("constructor")
        .assert()
        .failure()
        .stderr(predicates::str::contains(error_msg));
}

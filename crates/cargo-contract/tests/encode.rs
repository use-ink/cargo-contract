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
fn encode_works() {
    // given
    let contract = r#"
        #![cfg_attr(not(feature = "std"), no_std, no_main)]

        #[ink::contract]
        mod incrementer {
            #[ink(storage)]
            pub struct Incrementer {
                value: i32,
            }

            impl Incrementer {
                #[ink(constructor)]
                pub fn new(init_value: i32) -> Self {
                    Self { value: init_value }
                }

                #[ink(constructor)]
                pub fn new_default() -> Self {
                    Self::new(Default::default())
                }

                #[ink(message, selector = 0xBABABABA)]
                pub fn inc(&mut self, by: i32) {
                    self.value.saturating_add(by);
                }

                #[ink(message, selector = 0xCACACACA)]
                pub fn get(&self) -> i32 {
                    self.value
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
        .arg("incrementer")
        .assert()
        .success();

    let project_dir = tmp_dir.path().to_path_buf().join("incrementer");

    let lib = project_dir.join("lib.rs");
    std::fs::write(lib, contract).expect("Failed to write contract lib.rs");

    tracing::debug!("Building contract in {}", project_dir.to_string_lossy());
    cargo_contract(&project_dir).arg("build").assert().success();

    // when
    let output: &str = r#"Encoded data: BABABABA05000000"#;

    // then
    // message selector and data are being encoded properly
    cargo_contract(&project_dir)
        .arg("encode")
        .arg("--message")
        .arg("inc")
        .arg("--args")
        .arg("5")
        .assert()
        .success()
        .stdout(predicates::str::contains(output));

    // when
    let output: &str = r#"Encoded data: CACACACA"#;

    // then
    // message selector is being encoded properly
    cargo_contract(&project_dir)
        .arg("encode")
        .arg("--message")
        .arg("get")
        .assert()
        .success()
        .stdout(predicates::str::contains(output));
}

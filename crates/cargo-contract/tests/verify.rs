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

/// Compile the reference contract and return a byte array of its bundle.
fn compile_reference_contract() -> Vec<u8> {
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

            #[ink(message)]
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

    // cargo contract new reference contract
    cargo_contract(tmp_dir.path())
        .arg("new")
        .arg("incrementer")
        .assert()
        .success();

    let project_dir = tmp_dir.path().to_path_buf().join("incrementer");

    let lib = project_dir.join("lib.rs");
    std::fs::write(lib, contract).expect("Failed to write contract lib.rs");

    tracing::debug!("Building contract in {}", project_dir.to_string_lossy());
    cargo_contract(&project_dir)
        .arg("build")
        .arg("--release")
        .assert()
        .success();

    let bundle_path = project_dir.join("target/ink/incrementer.contract");

    std::fs::read(bundle_path)
        .expect("Failed to read the content of the contract bundle!")
}

#[test]
fn verify_equivalent_contracts() {
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

            #[ink(message)]
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

    // cargo contract new sample contract
    cargo_contract(tmp_dir.path())
        .arg("new")
        .arg("incrementer")
        .assert()
        .success();

    let project_dir = tmp_dir.path().to_path_buf().join("incrementer");

    let lib = project_dir.join("lib.rs");
    std::fs::write(lib, contract).expect("Failed to write contract lib.rs");

    // Compile reference contract and write bundle in the directory.
    let reference_contents = compile_reference_contract();
    let bundle = project_dir.join("reference.contract");
    std::fs::write(bundle, reference_contents)
        .expect("Failed to write bundle contract to the current dir!");

    // when
    let output: &str = r#""is_verified": true"#;

    // then
    cargo_contract(&project_dir)
        .arg("verify")
        .arg("reference.contract")
        .arg("--output-json")
        .assert()
        .success()
        .stdout(predicates::str::contains(output));
}

#[test]
fn verify_different_contracts() {
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

            #[ink(message)]
            pub fn inc(&mut self, by: i32) {
                self.value.saturating_add(by);
            }

            #[ink(message, selector = 0xCBCBCBCB)]
            pub fn get(&self) -> i32 {
                self.value
            }
        }
    }"#;

    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");

    // cargo contract new sample contract.
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

    // Compile reference contract and write bundle in the directory.
    let reference_contents = compile_reference_contract();
    let bundle = project_dir.join("reference.contract");
    std::fs::write(bundle, reference_contents)
        .expect("Failed to write bundle contract to the current dir!");

    // when
    let output: &str = r#"Failed to verify the authenticity of `incrementer`"#;

    // then
    cargo_contract(&project_dir)
        .arg("verify")
        .arg("reference.contract")
        .arg("--output-json")
        .assert()
        .failure()
        .stderr(predicates::str::contains(output));
}

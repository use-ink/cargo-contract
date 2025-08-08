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

mod utils;

use contract_build::project_path;
use serde_json::{
    Map,
    Value,
};
use std::path::PathBuf;
use tempfile::TempDir;

use utils::cargo_contract;

/// Compile the reference contract and return a byte array of its bundle and raw
/// binary.
fn compile_reference_contract() -> (Vec<u8>, Vec<u8>) {
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
                self.value = self.value.saturating_add(by);
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

    let target_dir = project_path(project_dir.join("target"));
    let bundle_path = target_dir.join("ink/incrementer.contract");
    let bundle = std::fs::read(bundle_path)
        .expect("Failed to read the content of the contract bundle!");

    let polkavm_path = target_dir.join("ink/incrementer.polkavm");
    let blob = std::fs::read(polkavm_path)
        .expect("Failed to read the content of the contract binary!");

    (bundle, blob)
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
                self.value = self.value.saturating_add(by);
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

    // Compile reference contract and write contract bundle and contract binary in the
    // directory.
    let (ref_bundle, ref_binary) = compile_reference_contract();
    let bundle = project_dir.join("reference.contract");
    std::fs::write(bundle, ref_bundle)
        .expect("Failed to write bundle contract to the current dir!");
    let binary_path = project_dir.join("reference.polkavm");
    std::fs::write(binary_path, ref_binary)
        .expect("Failed to write `.polkavm` binary to the current dir!");

    // when
    let output: &str = r#""is_verified": true"#;

    // then
    cargo_contract(&project_dir)
        .arg("verify")
        .arg("--contract-bundle")
        .arg("reference.contract")
        .arg("--output-json")
        .assert()
        .success()
        .stdout(predicates::str::contains(output));
    // and
    cargo_contract(&project_dir)
        .arg("verify")
        .arg("--contract-binary")
        .arg("reference.polkavm")
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
                self.value = self.value.saturating_add(by);
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

    // Compile reference contract and write contract bundle and contract binary in the
    // directory.
    let (ref_bundle, ref_binary) = compile_reference_contract();
    let bundle = project_dir.join("reference.contract");
    std::fs::write(bundle, ref_bundle)
        .expect("Failed to write bundle contract to the current dir!");
    let binary_path = project_dir.join("reference.polkavm");
    std::fs::write(binary_path, ref_binary)
        .expect("Failed to write polkavm binary to the current dir!");

    // when
    let output: &str = "Failed to verify `reference.contract` against the workspace at \
                        `Cargo.toml`: the hashed polkavm blobs are not matching.";

    // then
    cargo_contract(&project_dir)
        .arg("verify")
        .arg("--contract-bundle")
        .arg("reference.contract")
        .arg("--output-json")
        .assert()
        .failure()
        .stderr(predicates::str::contains(output));
    // and

    let output: &str = r#"Failed to verify the authenticity of the polkavm binary at `reference.polkavm`"#;
    cargo_contract(&project_dir)
        .arg("verify")
        .arg("--contract-binary")
        .arg("reference.polkavm")
        .arg("--output-json")
        .assert()
        .failure()
        .stderr(predicates::str::contains(output));
}

#[test]
fn verify_must_fail_on_manipulated_binary() {
    // given
    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");
    let (project_dir, mut metadata_json) = create_and_compile_minimal_contract(&tmp_dir);

    // when
    // we change the `source.polkavm` blob to a different contract binary, but the hash
    // will remain the same as the one from our compiled minimal contract.
    let source = metadata_json
        .get_mut("source")
        .expect("source field not found in metadata");
    let contract_binary = source
        .get_mut("contract_binary")
        .expect("`source.contract_binary` field not found in metadata");
    *contract_binary = Value::String(String::from("0x00"));

    let contract_file =
        project_dir.join("contract_with_mismatching_binary_hash_and_code.contract");
    let metadata = serde_json::to_string_pretty(&metadata_json)
        .expect("failed converting metadata to json");
    std::fs::write(contract_file, metadata)
        .expect("Failed to write bundle contract to the current dir!");

    // then
    let output: &str = "Failed to verify `contract_with_mismatching_binary_hash_and_code.contract` \
                        against the workspace at `Cargo.toml`: the hashed polkavm blobs are not \
                        matching.";
    cargo_contract(&project_dir)
        .arg("verify")
        .arg("--contract-bundle")
        .arg("contract_with_mismatching_binary_hash_and_code.contract")
        .arg("--output-json")
        .assert()
        .failure()
        .stderr(predicates::str::contains(output));
}

#[test]
fn verify_must_fail_on_corrupt_hash() {
    // given
    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");
    let (project_dir, mut metadata_json) = create_and_compile_minimal_contract(&tmp_dir);

    // when
    // we change the `source.hash` value to a different hash
    let source = metadata_json
        .get_mut("source")
        .expect("source field not found in metadata");
    let binary_hash = source
        .get_mut("hash")
        .expect("source.hash field not found in metadata");
    *binary_hash = Value::String(String::from(
        "0x0000000000000000000000000000000000000000000000000000000000000000",
    ));

    let contract_file = project_dir.join("contract_with_corrupt_hash.contract");
    let metadata = serde_json::to_string_pretty(&metadata_json)
        .expect("failed converting metadata to json");
    std::fs::write(contract_file, metadata)
        .expect("Failed to write bundle contract to the current dir!");

    // then
    let output: &str = "The reference contract `contract_with_corrupt_hash.contract` \
                        metadata is corrupt: the `source.hash` does not match the `source.polkavm` hash.";
    cargo_contract(&project_dir)
        .arg("verify")
        .arg("--contract-bundle")
        .arg("contract_with_corrupt_hash.contract")
        .arg("--output-json")
        .assert()
        .failure()
        .stderr(predicates::str::contains(output));
}

// Creates a minimal contract in `tmp_dir` and compiles it.
//
// Returns a tuple of:
//  * the workspace folder within `tmp_dir` and
//  * the metadata contained in the `.contract` file that build.
fn create_and_compile_minimal_contract(
    tmp_dir: &TempDir,
) -> (PathBuf, Map<String, Value>) {
    let contract = r#"
    #![cfg_attr(not(feature = "std"), no_std, no_main)]

    #[ink::contract]
    mod minimal {
        #[ink(storage)]
        pub struct Minimal {}

        impl Minimal {
            #[ink(constructor)]
            pub fn new() -> Self {
                Self { }
            }

            #[ink(message)]
            pub fn void(&self) { }
        }
    }"#;
    cargo_contract(tmp_dir.path())
        .arg("new")
        .arg("minimal")
        .assert()
        .success();
    let project_dir = tmp_dir.path().to_path_buf().join("minimal");
    let lib = project_dir.join("lib.rs");
    std::fs::write(lib, contract).expect("Failed to write contract lib.rs");

    tracing::debug!("Building contract in {}", project_dir.to_string_lossy());
    cargo_contract(&project_dir)
        .arg("build")
        .arg("--release")
        .assert()
        .success();

    let bundle_path =
        project_path(project_dir.clone().join("target")).join("ink/minimal.contract");
    let bundle = std::fs::read(&bundle_path).unwrap_or_else(|err| {
        panic!(
            "Failed to read the content of the contract bundle at {bundle_path:?}: {err:?}"
        );
    });
    let metadata_json: Map<String, Value> = serde_json::from_slice(&bundle).unwrap();

    (project_dir, metadata_json)
}

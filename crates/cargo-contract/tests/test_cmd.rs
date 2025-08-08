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

mod utils;

use crate::utils::node_proc::TestNodeProcess;
use utils::cargo_contract;

#[test]
fn test_cmd_works() {
    // Creates test project with "sol" ABI.
    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");
    let project_name = "test_subcommand";
    cargo_contract(tmp_dir.path())
        .arg("new")
        .arg(project_name)
        .arg("--abi=sol")
        .assert()
        .success();

    // Makes unit test compilation dependent on "sol" ABI `cfg` flag.
    let project_dir = tmp_dir.path().to_path_buf().join(project_name);
    let lib = project_dir.join("lib.rs");
    let contract = std::fs::read_to_string(&lib)
        .expect("Failed to read contract lib.rs")
        .replace("#[cfg(test)]", r#"#[cfg(all(ink_abi = "sol", test))]"#);
    std::fs::write(lib, contract).expect("Failed to write contract lib.rs");

    // Runs `cargo contract test` and verifies that unit tests are run successfully.
    cargo_contract(&project_dir)
        .arg("test")
        .assert()
        .success()
        .stdout(predicates::str::contains("it_works ... ok"));
}

/// Simple smoke test for basic contract interactions.
#[tokio::test]
async fn basic_contract_interactions_work_default_abi() {
    // Given
    let mut node = TestNodeProcess::<subxt::PolkadotConfig>::build_with_env_or_default()
        .spawn()
        .await
        .unwrap();

    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");
    let project_name = "interactions_test";
    cargo_contract(tmp_dir.path())
        .arg("new")
        .arg(project_name)
        .assert()
        .success();
    let project_dir = tmp_dir.path().to_path_buf().join(project_name);

    cargo_contract(&project_dir)
        .arg("build")
        .arg("--release")
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Your contract artifacts are ready.",
        ));

    // When
    let out = cargo_contract(&project_dir)
        .arg("instantiate")
        .args([
            "--suri",
            "//Alice",
            "--storage-deposit-limit",
            "100000000000000",
            "--manifest-path",
            project_dir.join("Cargo.toml").to_str().unwrap(),
            "--args",
            "true",
            "--constructor",
            "new",
            //--salt 1090
            "-x",
            "-y",
            "--url",
            node.url(),
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Event Revive ➜ Instantiated"));
    let out = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let re = regex::Regex::new(r"\s+Contract\s(0x[a-z0-9]+)\s").unwrap();
    let mat = re.captures(&out).unwrap();
    let mut iter = mat.iter();
    iter.next();
    let contract_addr = iter.next().unwrap().unwrap().as_str();
    tracing::debug!("Found contract address '{contract_addr}'");

    // Then
    cargo_contract(&project_dir)
        .arg("call")
        .args([
            "--suri",
            "//Alice",
            "--manifest-path",
            project_dir.join("Cargo.toml").to_str().unwrap(),
            "--contract",
            contract_addr,
            "--message",
            "get",
            "--url",
            node.url(),
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Result Ok(true)"));

    cargo_contract(&project_dir)
        .arg("call")
        .args([
            "--suri",
            "//Alice",
            "--storage-deposit-limit",
            "100000000000000",
            "--manifest-path",
            project_dir.join("Cargo.toml").to_str().unwrap(),
            "--contract",
            contract_addr,
            "--message",
            "flip",
            "-x",
            "-y",
            "--url",
            node.url(),
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("System ➜ ExtrinsicSuccess"));

    cargo_contract(&project_dir)
        .arg("call")
        .args([
            "--suri",
            "//Alice",
            "--manifest-path",
            project_dir.join("Cargo.toml").to_str().unwrap(),
            "--contract",
            contract_addr,
            "--message",
            "get",
            "--url",
            node.url(),
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Result Ok(false)"));

    node.kill().expect("child process could not be killed");
}

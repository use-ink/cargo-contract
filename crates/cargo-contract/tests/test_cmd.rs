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
    let output_default_works: &str = "default_works ... ok";
    let output_it_works: &str = "it_works ... ok";
    cargo_contract(&project_dir)
        .arg("test")
        .assert()
        .success()
        .stdout(predicates::str::contains(output_default_works))
        .stdout(predicates::str::contains(output_it_works));
}

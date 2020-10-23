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

use assert_cmd::Command;
use std::path::{Path, PathBuf};
use std::str;

fn cmd(path: &Path) -> Command {
    let mut cmd = Command::cargo_bin("cargo-contract").unwrap();
    cmd
        .current_dir(path)
        .arg("contract");
    cmd
}

/// Run the whole lifecycle of creating/building/
#[test]
fn build_deploy_instantiate_call() {
    // let tmp_dir = tempfile::Builder::new()
    //     .prefix("cargo-contract.cli.test.")
    //     .tempdir()
    //     .expect("temporary directory creation failed");
    //
    // // cargo contract new flipper
    // cmd(tmp_dir.path())
    //     .arg("new")
    //     .arg("flipper")
    //     .assert()
    //     .success();

    // cd flipper
    let mut project_path = PathBuf::from("/home/andrew/tmp/cargo-contract"); //tmp_dir.into_path();
    project_path.push("flipper");

    // cargo contract build
    // cmd(project_path.as_path())
    //     .arg("generate-metadata")
    //     .assert()
    //     .success();
    //
    // // cargo contract generate-metadata
    // cmd(project_path.as_path())
    //     .arg("generate-metadata")
    //     .assert()
    //     .success();

    // cargo contract deploy --suri //Alice
    let output =
        cmd(project_path.as_path())
            .arg("deploy")
            .args(&["--suri", "//Alice"])
            .output()
            .expect("failed to execute process");
    assert!(output.status.success(), "deploy failed");

    // Expected output:
    //   Code hash: 0x13118a4b9c3e3929f449051a023a64e6eaed7065843b1e719956df9dec68756a
    let regex = regex::Regex::new(".*Code hash: 0x([0-9A-Fa-f]+)").unwrap();
    let stdout = str::from_utf8(&output.stdout).unwrap();
    let caps = regex.captures(&stdout).unwrap();
    let code_hash = caps.get(1).unwrap().as_str();
    assert_eq!(64, code_hash.len());

    // cargo contract \
    //   instantiate new true \
    //   --code-hash <code_hash> \
    //   --endowment 100000000000000 \
    //   --suri //Alice
    let output =
        cmd(project_path.as_path())
            .arg("instantiate")
            .args(&["new", "true"])
            .args(&["--code-hash", code_hash])
            .args(&["--endowment", "100000000000000"])
            .args(&["--suri", "//Alice"])
            .output()
            .expect("failed to execute process");
    assert!(output.status.success(), "instantiate failed");
}

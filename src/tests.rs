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

use anyhow::Result;
use predicates::prelude::*;
use std::{
    io,
    ffi::OsStr,
    path::Path,
    process,
    str,
    thread,
    time,
};
use subxt::{Client, ClientBuilder, ContractsTemplateRuntime};

const CONTRACTS_NODE: &str = "canvas";

/// Create a `cargo contract` command
fn cargo_contract(path: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("cargo-contract").unwrap();
    cmd.current_dir(path).arg("contract");
    cmd
}

/// Spawn and manage an instance of a compatible contracts enabled chain node.
struct ContractsNodeProcess {
    proc: process::Child,
    client: Client<ContractsTemplateRuntime>,
}

impl Drop for ContractsNodeProcess {
    fn drop(&mut self) {
        if let Err(err) = self.proc.kill() {
            log::error!("Error killing contracts node {:?}: {}", self.proc, err)
        }
    }
}

impl ContractsNodeProcess {
    async fn spawn<S>(program: S) -> Result<Self>
    where
        S: AsRef<OsStr>
    {
        let mut proc = process::Command::new(program)
            .arg("-lruntime=debug")
            .arg("--dev")
            .spawn()?;
        // wait for rpc to be initialized
        let attempts = 0;
        let client =
            loop {
                thread::sleep(time::Duration::from_secs(1));
                let result = ClientBuilder::<ContractsTemplateRuntime>::new().build().await;
                if let Ok(client) = result {
                    break Ok(client);
                }
                if attempts < 10 {
                    continue;
                }
                if let Err(err) = result {
                    break Err(err)
                }
            };
        match client {
            Ok(client) => Ok(Self { proc, client }),
            Err(err) => {
                let err = anyhow::anyhow!("Failed to connect to node rpc after {} attempts: {}", attempts, err);
                log::error!("{}", err);
                proc.kill()?;
                Err(err)
            }
        }
    }
}

/// Sanity test the whole lifecycle of:
///   new -> build -> generate-metadata - deploy -> instantiate -> call
///
/// # Note
///
/// Requires running `--dev` node with compatible contracts pallet e.g.
/// https://github.com/paritytech/canvas-node.
///
/// Before running this test run `--purge-db`, and start the node.
#[async_std::test]
async fn build_deploy_instantiate_call() {
    env_logger::try_init().ok();

    let contracts_node = ContractsNodeProcess::spawn(CONTRACTS_NODE).await;
    assert!(contracts_node.is_ok());
}

    // let tmp_dir = tempfile::Builder::new()
    //     .prefix("cargo-contract.cli.test.")
    //     .tempdir()
    //     .expect("temporary directory creation failed");
    //
    // // cargo contract new flipper
    // cargo_contract(tmp_dir.path())
    //     .arg("new")
    //     .arg("flipper")
    //     .assert()
    //     .success();
    //
    // // cd flipper
    // let mut project_path = tmp_dir.into_path();
    // project_path.push("flipper");
    //
    // // build the contract
    // cargo_contract(project_path.as_path())
    //     .arg("generate-metadata")
    //     .assert()
    //     .success();
    //
    // // generate the contract metadata
    // cargo_contract(project_path.as_path())
    //     .arg("generate-metadata")
    //     .assert()
    //     .success();
    //
    // // upload the code blob to the chain
    // let output = cargo_contract(project_path.as_path())
    //     .arg("deploy")
    //     .args(&["--suri", "//Alice"])
    //     .output()
    //     .expect("failed to execute process");
    // assert!(output.status.success(), "deploy failed");
    //
    // // Expected output:
    // //   Code hash: 0x13118a4b9c3e3929f449051a023a64e6eaed7065843b1e719956df9dec68756a
    // let regex = regex::Regex::new("Code hash: 0x([0-9A-Fa-f]+)").unwrap();
    // let stdout = str::from_utf8(&output.stdout).unwrap();
    // let caps = regex.captures(&stdout).unwrap();
    // let code_hash = caps.get(1).unwrap().as_str();
    // assert_eq!(64, code_hash.len());
    //
    // // instantiate the contract with an initial value of true
    // let output = cargo_contract(project_path.as_path())
    //     .arg("instantiate")
    //     .args(&["new", "true"])
    //     .args(&["--code-hash", code_hash])
    //     .args(&["--endowment", "100000000000000"])
    //     .args(&["--suri", "//Alice"])
    //     .output()
    //     .expect("failed to execute process");
    // assert!(output.status.success(), "instantiate failed");
    //
    // // Expected output:l
    // //   Contract account: 5134f8a2fbfb03d09b19b8697b75dd72c5a5f41f69f095c6758e11f6f2e198d1 (5DuBUJbn...)
    // let regex = regex::Regex::new("Contract account: ([0-9A-Fa-f]+)").unwrap();
    // let stdout = str::from_utf8(&output.stdout).unwrap();
    // let caps = regex.captures(&stdout).unwrap();
    // let contract_account = caps.get(1).unwrap().as_str();
    // assert_eq!(64, contract_account.len());
    //
    // let call_get_rpc = |expected: bool| {
    //     cargo_contract(project_path.as_path())
    //         .arg("call")
    //         .arg("get")
    //         .arg("--rpc")
    //         .args(&["--contract", contract_account])
    //         .args(&["--suri", "//Alice"])
    //         .assert()
    //         .stdout(predicate::str::contains(expected.to_string()));
    // };
    //
    // // call the `get` message via rpc to assert that it was set to the initial value
    // call_get_rpc(true);
    //
    // // call the `flip` message with an extrinsic to change the state of the contract
    // cargo_contract(project_path.as_path())
    //     .arg("call")
    //     .arg("flip")
    //     .args(&["--contract", contract_account])
    //     .args(&["--suri", "//Alice"])
    //     .assert()
    //     .stdout(predicate::str::contains("ExtrinsicSuccess"));
    //
    // // call the `get` message via rpc to assert that the value has been flipped
    // call_get_rpc(false);
// }

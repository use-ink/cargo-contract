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

use crate::{
    CallCommandBuilder,
    CallExec,
    DisplayEvents,
    ExtrinsicOptsBuilder,
    InstantiateCommandBuilder,
    InstantiateExecResult,
    RemoveCommandBuilder,
    RemoveExec,
    UploadCommandBuilder,
    UploadExec,
};
use anyhow::Result;
use contract_build::code_hash;
use ink_env::DefaultEnvironment;
use predicates::prelude::*;
use std::{
    ffi::OsStr,
    path::Path,
    process,
    str,
    thread,
    time,
};
use subxt::{
    utils::H160,
    OnlineClient,
    PolkadotConfig as DefaultConfig,
};
use subxt_signer::{
    sr25519::Keypair,
    SecretUri,
};

const CONTRACTS_NODE: &str = "substrate-contracts-node";

/// Create a `cargo contract` command
fn cargo_contract(path: &Path) -> assert_cmd::Command {
    let mut cmd = assert_cmd::Command::cargo_bin("cargo-contract").unwrap();
    cmd.current_dir(path).arg("contract");
    cmd
}

// Find the contract address in the output
fn extract_contract_address(stdout: &str) -> &str {
    let regex = regex::Regex::new("Contract ([0-9A-Za-z]+)").unwrap();
    let caps = regex
        .captures(stdout)
        .expect("contract account regex capture");
    let contract_account = caps.get(1).unwrap().as_str();
    contract_account
}

/// Spawn and manage an instance of a compatible contracts enabled chain node.
#[allow(dead_code)]
struct ContractsNodeProcess {
    proc: process::Child,
    tmp_dir: tempfile::TempDir,
    client: OnlineClient<DefaultConfig>,
}

impl Drop for ContractsNodeProcess {
    fn drop(&mut self) {
        self.kill()
    }
}

impl ContractsNodeProcess {
    async fn spawn<S>(program: S) -> Result<Self>
    where
        S: AsRef<OsStr>,
    {
        let tmp_dir = tempfile::Builder::new()
            .prefix("cargo-contract.cli.test.node")
            .tempdir()?;

        let mut proc = process::Command::new(program)
            .env("RUST_LOG", "error")
            .arg("--dev")
            .arg(format!("--base-path={}", tmp_dir.path().to_string_lossy()))
            .spawn()?;
        // wait for rpc to be initialized
        const MAX_ATTEMPTS: u32 = 15;
        let mut attempts = 1;
        let client = loop {
            thread::sleep(time::Duration::from_secs(1));
            tracing::debug!(
                "Connecting to contracts enabled node, attempt {}/{}",
                attempts,
                MAX_ATTEMPTS
            );
            let result = OnlineClient::new().await;
            if let Ok(client) = result {
                break Ok(client)
            }
            if attempts < MAX_ATTEMPTS {
                attempts += 1;
                continue
            }
            if let Err(err) = result {
                break Err(err)
            }
        };
        match client {
            Ok(client) => {
                Ok(Self {
                    proc,
                    client,
                    tmp_dir,
                })
            }
            Err(err) => {
                let err = anyhow::anyhow!(
                    "Failed to connect to node rpc after {} attempts: {}",
                    attempts,
                    err
                );
                tracing::error!("{}", err);
                proc.kill()?;
                Err(err)
            }
        }
    }

    fn kill(&mut self) {
        tracing::debug!("Killing contracts node process {}", self.proc.id());
        if let Err(err) = self.proc.kill() {
            tracing::error!(
                "Error killing contracts node process {}: {}",
                self.proc.id(),
                err
            )
        }
    }
}

/// Init a tracing subscriber for logging in tests.
///
/// Be aware that this enables `TRACE` by default. It also ignores any error
/// while setting up the logger.
///
/// The logs are not shown by default, logs are only shown when the test fails
/// or if [`nocapture`](https://doc.rust-lang.org/cargo/commands/cargo-test.html#display-options)
/// is being used.
#[cfg(any(feature = "integration-tests", feature = "test-ci-only"))]
pub fn init_tracing_subscriber() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_test_writer()
        .try_init();
}

/// Sanity test the whole lifecycle of:
///   new -> build -> upload -> instantiate -> call
///
/// # Note
///
/// Requires [`substrate-contracts-node`](https://github.com/paritytech/substrate-contracts-node/) to
/// be installed and available on the `PATH`, and the no other process running using the
/// default port `9944`.
#[tokio::test]
async fn build_upload_instantiate_call() {
    init_tracing_subscriber();

    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");

    cargo_contract(tmp_dir.path())
        .arg("new")
        .arg("flipper")
        .assert()
        .success();

    let mut project_path = tmp_dir.path().to_path_buf();
    project_path.push("flipper");

    cargo_contract(project_path.as_path())
        .arg("build")
        .arg("--target")
        .arg("riscv")
        .assert()
        .success();

    let node_process = ContractsNodeProcess::spawn(CONTRACTS_NODE)
        .await
        .expect("Error spawning contracts node");

    let output = cargo_contract(project_path.as_path())
        .arg("upload")
        .args(["--suri", "//Alice"])
        .arg("-x")
        .output()
        .expect("failed to execute process");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(output.status.success(), "upload code failed: {stderr}");

    // TODO - add tests for `suri-path` and `password-path`

    let output = cargo_contract(project_path.as_path())
        .arg("instantiate")
        .args(["--constructor", "new"])
        .args(["--args", "true"])
        .args(["--suri", "//Alice"])
        .arg("-x")
        .output()
        .expect("failed to execute process");
    let stdout = str::from_utf8(&output.stdout).unwrap();
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(output.status.success(), "instantiate failed: {stderr}");

    let contract_account = extract_contract_address(stdout);
    assert_eq!(48, contract_account.len(), "{stdout:?}");

    let call_get_rpc = |expected: bool| {
        cargo_contract(project_path.as_path())
            .arg("call")
            .args(["--message", "get"])
            .args(["--contract", contract_account])
            .args(["--suri", "//Alice"])
            .assert()
            .stdout(predicate::str::contains(expected.to_string()));
    };

    call_get_rpc(true);

    cargo_contract(project_path.as_path())
        .arg("call")
        .args(["--message", "flip"])
        .args(["--contract", contract_account])
        .args(["--suri", "//Alice"])
        .arg("-x")
        .assert()
        .stdout(predicate::str::contains("ExtrinsicSuccess"));

    call_get_rpc(false);

    // prevent the node_process from being dropped and killed
    let _ = node_process;
}

/// Sanity test the whole lifecycle of:
/// build -> upload -> remove
#[tokio::test]
async fn build_upload_remove() {
    init_tracing_subscriber();

    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");

    cargo_contract(tmp_dir.path())
        .arg("new")
        .arg("incrementer")
        .assert()
        .success();

    let mut project_path = tmp_dir.path().to_path_buf();
    project_path.push("incrementer");

    cargo_contract(project_path.as_path())
        .arg("build")
        .assert()
        .success();

    let node_process = ContractsNodeProcess::spawn(CONTRACTS_NODE)
        .await
        .expect("Error spawning contracts node");

    let output = cargo_contract(project_path.as_path())
        .arg("upload")
        .args(["--suri", "//Alice"])
        .arg("-x")
        .output()
        .expect("failed to execute process");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(output.status.success(), "upload code failed: {stderr}");

    let stdout = str::from_utf8(&output.stdout).unwrap();

    // find the code hash in the output
    let regex = regex::Regex::new("0x([0-9A-Fa-f]+)").unwrap();
    let caps = regex.captures(stdout).expect("Failed to find codehash");
    let code_hash = caps.get(1).unwrap().as_str();
    assert_eq!(64, code_hash.len());

    let output = cargo_contract(project_path.as_path())
        .arg("remove")
        .args(["--suri", "//Alice"])
        .args(["--code-hash", code_hash])
        .output()
        .expect("failed to execute process");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(output.status.success(), "remove failed: {stderr}");

    // prevent the node_process from being dropped and killed
    let _ = node_process;
}

/// Sanity test the whole lifecycle of:
///   new -> build -> upload -> instantiate -> info
///
/// # Note
///
/// Requires [`substrate-contracts-node`](https://github.com/paritytech/substrate-contracts-node/) to
/// be installed and available on the `PATH`, and the no other process running using the
/// default port `9944`.
#[tokio::test]
async fn build_upload_instantiate_info() {
    init_tracing_subscriber();

    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");

    cargo_contract(tmp_dir.path())
        .arg("new")
        .arg("flipper")
        .assert()
        .success();

    let mut project_path = tmp_dir.path().to_path_buf();
    project_path.push("flipper");

    cargo_contract(project_path.as_path())
        .arg("build")
        .arg("--target")
        .arg("riscv")
        .assert()
        .success();

    let node_process = ContractsNodeProcess::spawn(CONTRACTS_NODE)
        .await
        .expect("Error spawning contracts node");

    let output = cargo_contract(project_path.as_path())
        .arg("upload")
        .args(["--suri", "//Alice"])
        .arg("-x")
        .output()
        .expect("failed to execute process");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(output.status.success(), "upload code failed: {stderr}");

    let output = cargo_contract(project_path.as_path())
        .arg("instantiate")
        .args(["--constructor", "new"])
        .args(["--args", "true"])
        .args(["--suri", "//Alice"])
        .arg("-x")
        .output()
        .expect("failed to execute process");
    let stdout = str::from_utf8(&output.stdout).unwrap();
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(output.status.success(), "instantiate failed: {stderr}");

    let contract_account = extract_contract_address(stdout);
    assert_eq!(48, contract_account.len(), "{stdout:?}");

    let output = cargo_contract(project_path.as_path())
        .arg("info")
        .args(["--contract", contract_account])
        .output()
        .expect("failed to execute process");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(output.status.success(), "getting info failed: {stderr}");

    let output = cargo_contract(project_path.as_path())
        .arg("info")
        .args(["--contract", contract_account])
        .arg("--output-json")
        .output()
        .expect("failed to execute process");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(
        output.status.success(),
        "getting info as JSON format failed: {stderr}"
    );

    let output = cargo_contract(project_path.as_path())
        .arg("info")
        .args(["--contract", contract_account])
        .arg("--binary")
        .output()
        .expect("failed to execute process");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(
        output.status.success(),
        "getting Wasm code failed: {stderr}"
    );

    // construct the contract file path
    let contract_wasm = project_path.join("target/ink/flipper.wasm");

    let code = std::fs::read(contract_wasm).expect("contract Wasm file not found");
    assert_eq!(code_hash(&code), code_hash(&output.stdout));

    cargo_contract(project_path.as_path())
        .arg("info")
        .args(["--contract", contract_account])
        .arg("--output-json")
        .arg("--binary")
        .assert()
        .stdout(predicate::str::contains(r#""wasm": "0x"#));

    let output = cargo_contract(project_path.as_path())
        .arg("info")
        .arg("--all")
        .output()
        .expect("failed to execute process");
    let stdout = str::from_utf8(&output.stdout).unwrap();
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(
        output.status.success(),
        "getting all contracts failed: {stderr}"
    );

    assert_eq!(stdout.trim_end(), contract_account, "{stdout:?}");

    // prevent the node_process from being dropped and killed
    let _ = node_process;
}

/// This test uses contract extrinsics API to test the whole lifecycle of:
///   new -> build -> upload -> instantiate -> call
///
/// # Note
///
/// Requires [`substrate-contracts-node`](https://github.com/paritytech/substrate-contracts-node/) to
/// be installed and available on the `PATH`, and the no other process running using the
/// default port `9944`.
#[tokio::test]
async fn api_build_upload_instantiate_call() {
    init_tracing_subscriber();

    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");

    cargo_contract(tmp_dir.path())
        .arg("new")
        .arg("flipper")
        .assert()
        .success();

    let mut project_path = tmp_dir.path().to_path_buf();
    project_path.push("flipper");

    cargo_contract(project_path.as_path())
        .arg("build")
        .arg("--target")
        .arg("riscv")
        .assert()
        .success();

    let node_process = ContractsNodeProcess::spawn(CONTRACTS_NODE)
        .await
        .expect("Error spawning contracts node");

    // construct the contract file path
    let contract_file = project_path.join("target/ink/flipper.contract");

    // upload the contract
    let uri = <SecretUri as std::str::FromStr>::from_str("//Alice").unwrap();
    let signer = Keypair::from_uri(&uri).unwrap();
    let opts = ExtrinsicOptsBuilder::new(signer)
        .file(Some(contract_file))
        .done();
    let upload: UploadExec<DefaultConfig, DefaultEnvironment, Keypair> =
        UploadCommandBuilder::new(opts.clone())
            .done()
            .await
            .unwrap();
    let upload_result = upload.upload_code().await;
    assert!(upload_result.is_ok(), "upload code failed");
    upload_result.unwrap();

    // instantiate the contract
    let instantiate = InstantiateCommandBuilder::new(opts.clone())
        .constructor("new")
        .args(["true"].to_vec())
        .done()
        .await
        .unwrap();
    let instantiate_result = instantiate.instantiate(None).await;
    assert!(instantiate_result.is_ok(), "instantiate code failed");
    let instantiate_result: InstantiateExecResult<DefaultConfig, H160> =
        instantiate_result.unwrap();
    let contract_account = instantiate_result.contract_address.to_string();
    assert_eq!(48, contract_account.len(), "{contract_account:?}");

    // call the contract
    // the value should be true
    let call: CallExec<DefaultConfig, DefaultEnvironment, Keypair> =
        CallCommandBuilder::new(instantiate_result.contract_address, "get", opts.clone())
            .done()
            .await
            .unwrap();
    let result = call.call_dry_run().await;
    assert!(result.is_ok(), "call failed");
    let result = result.unwrap();
    let ret_val = result.result.unwrap();
    let value = call
        .transcoder()
        .decode_message_return(call.message(), &mut &ret_val.data[..])
        .unwrap()
        .to_string();
    assert!(value.contains("true"), "{:#?}", value);

    // call the contract on the immutable "get" message trying to execute
    // this should fail because "get" is immutable
    match call.call(None).await {
        Err(crate::ErrorVariant::Generic(_)) => {}
        _ => panic!("immutable call was not prevented"),
    }

    // call the contract
    // flip the value
    let call: CallExec<DefaultConfig, DefaultEnvironment, Keypair> =
        CallCommandBuilder::new(
            instantiate_result.contract_address,
            "flip",
            opts.clone(),
        )
        .done()
        .await
        .unwrap();
    let call_result = call.call(None).await;
    assert!(call_result.is_ok(), "call failed");
    let call_result = call_result.unwrap();
    let output = DisplayEvents::from_events::<DefaultConfig, DefaultEnvironment>(
        &call_result,
        None,
        &call.client().metadata(),
    )
    .unwrap()
    .to_json()
    .unwrap();
    assert!(output.contains("ExtrinsicSuccess"), "{:#?}", output);

    // call the contract
    // make sure the value has been flipped
    let call: CallExec<DefaultConfig, DefaultEnvironment, Keypair> =
        CallCommandBuilder::new(instantiate_result.contract_address, "get", opts.clone())
            .done()
            .await
            .unwrap();
    let result = call.call_dry_run().await;
    assert!(result.is_ok(), "call failed");
    let result = result.unwrap();
    let ret_val = result.result.unwrap();
    let value = call
        .transcoder()
        .decode_message_return(call.message(), &mut &ret_val.data[..])
        .unwrap()
        .to_string();
    assert!(value.contains("false"), "{:#?}", value);

    // prevent the node_process from being dropped and killed
    let _ = node_process;
}

/// Sanity test the whole lifecycle of:
/// build -> upload -> remove
#[tokio::test]
async fn api_build_upload_remove() {
    init_tracing_subscriber();

    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");

    cargo_contract(tmp_dir.path())
        .arg("new")
        .arg("incrementer")
        .assert()
        .success();

    let mut project_path = tmp_dir.path().to_path_buf();
    project_path.push("incrementer");

    cargo_contract(project_path.as_path())
        .arg("build")
        .arg("--target")
        .arg("riscv")
        .assert()
        .success();

    let node_process = ContractsNodeProcess::spawn(CONTRACTS_NODE)
        .await
        .expect("Error spawning contracts node");

    // construct the contract file path
    let contract_file = project_path.join("target/ink/incrementer.contract");

    // upload the contract
    let uri = <SecretUri as std::str::FromStr>::from_str("//Alice").unwrap();
    let signer = Keypair::from_uri(&uri).unwrap();
    let opts = ExtrinsicOptsBuilder::new(signer)
        .file(Some(contract_file))
        .done();
    let upload: UploadExec<DefaultConfig, DefaultEnvironment, Keypair> =
        UploadCommandBuilder::new(opts.clone())
            .done()
            .await
            .unwrap();
    let upload_result = upload.upload_code().await;
    assert!(upload_result.is_ok(), "upload code failed");
    let upload_result = upload_result.unwrap();
    let code_hash_h256 = upload_result.code_stored.unwrap().code_hash;
    let code_hash = hex::encode(code_hash_h256);
    assert_eq!(64, code_hash.len(), "{code_hash:?}");

    // remove the contract
    let remove: RemoveExec<DefaultConfig, DefaultEnvironment, Keypair> =
        RemoveCommandBuilder::new(opts.clone())
            .code_hash(Some(code_hash_h256))
            .done()
            .await
            .unwrap();
    let remove_result = remove.remove_code().await;
    assert!(remove_result.is_ok(), "remove code failed");
    remove_result.unwrap();

    // prevent the node_process from being dropped and killed
    let _ = node_process;
}

/// Sanity test the RPC API
#[tokio::test]
async fn api_rpc_call() {
    init_tracing_subscriber();

    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");

    let node_process = ContractsNodeProcess::spawn(CONTRACTS_NODE)
        .await
        .expect("Error spawning contracts node");

    cargo_contract(tmp_dir.path())
        .arg("rpc")
        .arg("author_insertKey")
        .arg("\"sr25\"")
        .arg("\"//ALICE\"")
        .arg("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY")
        .assert()
        .success();

    let output = cargo_contract(tmp_dir.path())
        .arg("rpc")
        .arg("author_hasKey")
        .arg("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY")
        .arg("\"sr25\"")
        .arg("--output-json")
        .output()
        .expect("failed to execute process");

    let stdout = str::from_utf8(&output.stdout).unwrap();
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(
        output.status.success(),
        "rpc method execution failed: {stderr}"
    );

    assert_eq!(stdout.trim_end(), "true", "{stdout:?}");

    // prevent the node_process from being dropped and killed
    let _ = node_process;
}

/// Sanity test the whole lifecycle of:
///   new -> build -> upload -> instantiate -> storage
///
/// # Note
///
/// Requires [`substrate-contracts-node`](https://github.com/paritytech/substrate-contracts-node/) to
/// be installed and available on the `PATH`, and the no other process running using the
/// default port `9944`.
#[tokio::test]
async fn build_upload_instantiate_storage() {
    init_tracing_subscriber();

    let tmp_dir = tempfile::Builder::new()
        .prefix("cargo-contract.cli.test.")
        .tempdir()
        .expect("temporary directory creation failed");

    cargo_contract(tmp_dir.path())
        .arg("new")
        .arg("flipper")
        .assert()
        .success();

    let mut project_path = tmp_dir.path().to_path_buf();
    project_path.push("flipper");

    cargo_contract(project_path.as_path())
        .arg("build")
        .arg("--target")
        .arg("riscv")
        .assert()
        .success();

    let node_process = ContractsNodeProcess::spawn(CONTRACTS_NODE)
        .await
        .expect("Error spawning contracts node");

    let output = cargo_contract(project_path.as_path())
        .arg("upload")
        .args(["--suri", "//Alice"])
        .arg("-x")
        .output()
        .expect("failed to execute process");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(output.status.success(), "upload code failed: {stderr}");

    let output = cargo_contract(project_path.as_path())
        .arg("instantiate")
        .args(["--constructor", "new"])
        .args(["--args", "true"])
        .args(["--suri", "//Alice"])
        .arg("-x")
        .output()
        .expect("failed to execute process");
    let stdout = str::from_utf8(&output.stdout).unwrap();
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(output.status.success(), "instantiate failed: {stderr}");

    let contract_account = extract_contract_address(stdout);
    assert_eq!(48, contract_account.len(), "{stdout:?}");

    let output = cargo_contract(project_path.as_path())
        .arg("storage")
        .args(["--contract", contract_account])
        .arg("--raw")
        .output()
        .expect("failed to execute process");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(
        output.status.success(),
        "getting storage as raw format failed: {stderr}"
    );

    let contract_manifest = project_path.join("Cargo.toml");
    let contract_manifest = contract_manifest.to_str().unwrap();

    let output = cargo_contract(project_path.as_path())
        .arg("storage")
        .args(["--contract", contract_account])
        .args(["--manifest-path", contract_manifest])
        .arg("--output-json")
        .output()
        .expect("failed to execute process");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(
        output.status.success(),
        "getting storage as JSON format failed: {stderr}"
    );

    let output = cargo_contract(project_path.as_path())
        .arg("storage")
        .args(["--contract", contract_account])
        .args(["--manifest-path", contract_manifest])
        .output()
        .expect("failed to execute process");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(
        output.status.success(),
        "getting storage as table failed: {stderr}"
    );

    // prevent the node_process from being dropped and killed
    let _ = node_process;
}

// Copyright 2018-2022 Parity Technologies (UK) Ltd.
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

#![deny(unused_crate_dependencies)]

mod cmd;

use self::cmd::{
    BuildCommand,
    CallCommand,
    CheckCommand,
    DecodeCommand,
    ErrorVariant,
    InfoCommand,
    InstantiateCommand,
    RemoveCommand,
    UploadCommand,
};
use cmd::encode::EncodeCommand;
use contract_build::{
    util::DEFAULT_KEY_COL_WIDTH,
    OutputType,
};
use std::{
    fmt::Debug,
    path::PathBuf,
    str::FromStr,
};
use tokio::runtime::Runtime;

use anyhow::{
    anyhow,
    Context,
    Error,
    Result,
};
use clap::{
    Args,
    Parser,
    Subcommand,
};
use colored::Colorize;
use contract_build::name_value_println;
use contract_extrinsics::{
    display_contract_exec_result,
    display_contract_exec_result_debug,
    display_dry_run_result_warning,
    print_dry_running_status,
    print_gas_required_success,
    prompt_confirm_tx,
    CallDryRunResult,
    CallExec,
    Code,
    CodeHashResult,
    InstantiateExec,
    StorageDeposit,
    TokenMetadata,
    UploadDryRunResult,
    MAX_KEY_COL_WIDTH,
};
use sp_weights::Weight;

// These crates are only used when we run integration tests `--features
// integration-tests`. However since we can't have optional `dev-dependencies` we pretend
// to use them during normal test runs in order to satisfy the `unused_crate_dependencies`
// lint.
#[cfg(test)]
use assert_cmd as _;
#[cfg(test)]
use predicates as _;
#[cfg(test)]
use regex as _;
#[cfg(test)]
use tempfile as _;

// Only used on windows.
use which as _;

#[derive(Debug, Parser)]
#[clap(bin_name = "cargo")]
#[clap(version = env!("CARGO_CONTRACT_CLI_IMPL_VERSION"))]
pub(crate) enum Opts {
    /// Utilities to develop Wasm smart contracts.
    #[clap(name = "contract")]
    #[clap(version = env!("CARGO_CONTRACT_CLI_IMPL_VERSION"))]
    #[clap(action = ArgAction::DeriveDisplayOrder)]
    Contract(ContractArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ContractArgs {
    #[clap(subcommand)]
    cmd: Command,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct HexData(pub Vec<u8>);

impl FromStr for HexData {
    type Err = hex::FromHexError;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        hex::decode(input).map(HexData)
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Setup and create a new smart contract project
    #[clap(name = "new")]
    New {
        /// The name of the newly created smart contract
        name: String,
        /// The optional target directory for the contract project
        #[clap(short, long, value_parser)]
        target_dir: Option<PathBuf>,
    },
    /// Compiles the contract, generates metadata, bundles both together in a
    /// `<name>.contract` file
    #[clap(name = "build")]
    Build(BuildCommand),
    /// Check that the code builds as Wasm; does not output any `<name>.contract`
    /// artifact to the `target/` directory
    #[clap(name = "check")]
    Check(CheckCommand),
    /// Upload contract code
    #[clap(name = "upload")]
    Upload(UploadCommand),
    /// Instantiate a contract
    #[clap(name = "instantiate")]
    Instantiate(InstantiateCommand),
    /// Call a contract
    #[clap(name = "call")]
    Call(CallCommand),
    /// Encodes a contracts input calls and their arguments
    #[clap(name = "encode")]
    Encode(EncodeCommand),
    /// Decodes a contracts input or output data (supplied in hex-encoding)
    #[clap(name = "decode")]
    Decode(DecodeCommand),
    /// Remove contract code
    #[clap(name = "remove")]
    Remove(RemoveCommand),
    /// Display information about a contract
    #[clap(name = "info")]
    Info(InfoCommand),
}

fn main() {
    tracing_subscriber::fmt::init();

    let Opts::Contract(args) = Opts::parse();

    match exec(args.cmd) {
        Ok(()) => {}
        Err(err) => {
            eprintln!("{err:?}");
            std::process::exit(1);
        }
    }
}

fn exec(cmd: Command) -> Result<()> {
    match &cmd {
        Command::New { name, target_dir } => {
            contract_build::new_contract_project(name, target_dir.as_ref())?;
            println!("Created contract {name}");
            Ok(())
        }
        Command::Build(build) => {
            let result = build.exec().map_err(format_err)?;

            if matches!(result.output_type, OutputType::Json) {
                println!("{}", result.serialize_json()?)
            } else if result.verbosity.is_verbose() {
                println!("{}", result.display())
            }
            Ok(())
        }
        Command::Check(check) => {
            let res = check.exec().map_err(format_err)?;
            assert!(
                res.dest_wasm.is_none(),
                "no dest_wasm must be on the generation result"
            );
            Ok(())
        }
        Command::Upload(upload) => {
            handle_upload(upload).map_err(|err| map_extrinsic_err(err, upload.is_json()))
        }
        Command::Instantiate(instantiate) => {
            handle_instantiate(instantiate)
                .map_err(|err| map_extrinsic_err(err, instantiate.is_json()))
        }
        Command::Call(call) => {
            handle_call(call).map_err(|err| map_extrinsic_err(err, call.is_json()))
        }
        Command::Encode(encode) => encode.run().map_err(format_err),
        Command::Decode(decode) => decode.run().map_err(format_err),
        Command::Remove(remove) => {
            handle_remove(remove).map_err(|err| map_extrinsic_err(err, remove.is_json()))
        }
        Command::Info(info) => info.run().map_err(format_err),
    }
}

fn handle_upload(upload_command: &UploadCommand) -> Result<(), ErrorVariant> {
    Runtime::new()?.block_on(async {
        let upload_exec = upload_command.preprocess().await?;
        let code_hash = upload_exec.code().code_hash();

        if !upload_exec.opts().execute() {
            match upload_exec.upload_code_rpc().await? {
                Ok(result) => {
                    let upload_result = UploadDryRunResult {
                        result: String::from("Success!"),
                        code_hash: format!("{:?}", result.code_hash),
                        deposit: result.deposit,
                    };
                    if upload_exec.output_json() {
                        println!("{}", upload_result.to_json()?);
                    } else {
                        upload_result.print();
                        display_dry_run_result_warning("upload");
                    }
                }
                Err(err) => {
                    let metadata = upload_exec.client().metadata();
                    let err = ErrorVariant::from_dispatch_error(&err, &metadata)?;
                    if upload_exec.output_json() {
                        return Err(err)
                    } else {
                        name_value_println!("Result", err);
                    }
                }
            }
        } else {
            let upload_result = upload_exec.upload_code().await?;
            let display_events = upload_result.display_events;
            let output = if upload_exec.output_json() {
                display_events.to_json()?
            } else {
                let token_metadata = TokenMetadata::query(upload_exec.client()).await?;
                display_events
                    .display_events(upload_exec.opts().verbosity()?, &token_metadata)?
            };
            println!("{output}");
            if let Some(code_stored) = upload_result.code_stored {
                let upload_result = CodeHashResult {
                    code_hash: format!("{:?}", code_stored.code_hash),
                };
                if upload_exec.output_json() {
                    println!("{}", upload_result.to_json()?);
                } else {
                    upload_result.print();
                }
            } else {
                let code_hash = hex::encode(code_hash);
                return Err(anyhow::anyhow!(
                "This contract has already been uploaded with code hash: 0x{code_hash}"
            )
                .into())
            }
        }
        Ok(())
    })
}

fn handle_instantiate(
    instantiate_command: &InstantiateCommand,
) -> Result<(), ErrorVariant> {
    Runtime::new()?.block_on(async {
        let instantiate_exec = instantiate_command.preprocess().await?;

        if !instantiate_exec.opts().execute() {
            let result = instantiate_exec.instantiate_dry_run().await?;
            match instantiate_exec.simulate_instantiation().await {
                Ok(dry_run_result) => {
                    if instantiate_exec.output_json() {
                        println!("{}", dry_run_result.to_json()?);
                    } else {
                        dry_run_result.print();
                        display_contract_exec_result_debug::<_, DEFAULT_KEY_COL_WIDTH>(
                            &result,
                        )?;
                        display_dry_run_result_warning("instantiate");
                    }
                    Ok(())
                }
                Err(object) => {
                    if instantiate_exec.output_json() {
                        return Err(object)
                    } else {
                        name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                        display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(&result)?;
                    }
                    Err(object)
                }
            }
        } else {
            tracing::debug!("instantiate data {:?}", instantiate_exec.args().data());
            let gas_limit =
                pre_submit_dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
            if !instantiate_exec.opts().skip_confirm() {
                prompt_confirm_tx(|| {
                    instantiate_exec.print_default_instantiate_preview(gas_limit);
                    if let Code::Existing(code_hash) =
                        instantiate_exec.args().code().clone()
                    {
                        name_value_println!(
                            "Code hash",
                            format!("{code_hash:?}"),
                            DEFAULT_KEY_COL_WIDTH
                        );
                    }
                })?;
            }
            let instantiate_result =
                instantiate_exec.instantiate(Some(gas_limit)).await?;
            instantiate_exec.display_result(instantiate_result).await?;
            Ok(())
        }
    })
}

fn handle_call(call_command: &CallCommand) -> Result<(), ErrorVariant> {
    Runtime::new()?.block_on(async {
        let call_exec = call_command.preprocess().await?;
        if !call_exec.opts().execute() {
            let result = call_exec.call_dry_run().await?;
            match result.result {
                Ok(ref ret_val) => {
                    let value = call_exec
                        .transcoder()
                        .decode_message_return(
                            call_exec.message(),
                            &mut &ret_val.data[..],
                        )
                        .context(format!(
                            "Failed to decode return value {:?}",
                            &ret_val
                        ))?;
                    let dry_run_result = CallDryRunResult {
                        reverted: ret_val.did_revert(),
                        data: value,
                        gas_consumed: result.gas_consumed,
                        gas_required: result.gas_required,
                        storage_deposit: StorageDeposit::from(&result.storage_deposit),
                    };
                    if call_exec.output_json() {
                        println!("{}", dry_run_result.to_json()?);
                    } else {
                        dry_run_result.print();
                        display_contract_exec_result_debug::<_, DEFAULT_KEY_COL_WIDTH>(
                            &result,
                        )?;
                        display_dry_run_result_warning("message");
                    };
                }
                Err(ref err) => {
                    let metadata = call_exec.client().metadata();
                    let object = ErrorVariant::from_dispatch_error(err, &metadata)?;
                    if call_exec.output_json() {
                        return Err(object)
                    } else {
                        name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                        display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(&result)?;
                    }
                }
            }
        } else {
            let gas_limit = pre_submit_dry_run_gas_estimate_call(&call_exec).await?;
            if !call_exec.opts().skip_confirm() {
                prompt_confirm_tx(|| {
                    name_value_println!(
                        "Message",
                        call_exec.message(),
                        DEFAULT_KEY_COL_WIDTH
                    );
                    name_value_println!(
                        "Args",
                        call_exec.args().join(" "),
                        DEFAULT_KEY_COL_WIDTH
                    );
                    name_value_println!(
                        "Gas limit",
                        gas_limit.to_string(),
                        DEFAULT_KEY_COL_WIDTH
                    );
                })?;
            }
            let token_metadata = TokenMetadata::query(call_exec.client()).await?;
            let display_events = call_exec.call(Some(gas_limit)).await?;
            let output = if call_exec.output_json() {
                display_events.to_json()?
            } else {
                display_events
                    .display_events(call_exec.opts().verbosity()?, &token_metadata)?
            };
            println!("{output}");
        }
        Ok(())
    })
}

fn handle_remove(remove_command: &RemoveCommand) -> Result<(), ErrorVariant> {
    Runtime::new()?.block_on(async {
        let remove_exec = remove_command.preprocess().await?;
        let remove_result = remove_exec.remove_code().await?;
        let display_events = remove_result.display_events;
        let output = if remove_exec.output_json() {
            display_events.to_json()?
        } else {
            let token_metadata = TokenMetadata::query(remove_exec.client()).await?;
            display_events
                .display_events(remove_exec.opts().verbosity()?, &token_metadata)?
        };
        println!("{output}");
        if let Some(code_removed) = remove_result.code_removed {
            let remove_result = code_removed.code_hash;

            if remove_exec.output_json() {
                println!("{}", &remove_result);
            } else {
                name_value_println!("Code hash", format!("{remove_result:?}"));
            }
            Result::<(), ErrorVariant>::Ok(())
        } else {
            let error_code_hash = hex::encode(remove_exec.final_code_hash());
            Err(anyhow::anyhow!(
                "Error removing the code for the supplied code hash: {}",
                error_code_hash
            )
            .into())
        }
    })
}

fn map_extrinsic_err(err: ErrorVariant, is_json: bool) -> Error {
    if is_json {
        anyhow!(
            "{}",
            serde_json::to_string_pretty(&err)
                .expect("error serialization is infallible; qed")
        )
    } else {
        format_err(err)
    }
}

fn format_err<E: Debug>(err: E) -> Error {
    anyhow!(
        "{} {}",
        "ERROR:".bright_red().bold(),
        format!("{err:?}").bright_red()
    )
}

/// A helper function to estimate the gas required for a contract instantiation.
pub async fn pre_submit_dry_run_gas_estimate_instantiate(
    instantiate_exec: &InstantiateExec,
) -> Result<Weight> {
    if instantiate_exec.opts().skip_dry_run() {
        return match (instantiate_exec.args().gas_limit(), instantiate_exec.args().proof_size()) {
                (Some(ref_time), Some(proof_size)) => Ok(Weight::from_parts(ref_time, proof_size)),
                _ => {
                    Err(anyhow!(
                        "Weight args `--gas` and `--proof-size` required if `--skip-dry-run` specified"
                    ))
                }
            };
    }
    if !instantiate_exec.output_json() {
        print_dry_running_status(instantiate_exec.args().constructor());
    }
    let instantiate_result = instantiate_exec.instantiate_dry_run().await?;
    match instantiate_result.result {
        Ok(_) => {
            if !instantiate_exec.output_json() {
                print_gas_required_success(instantiate_result.gas_required);
            }
            // use user specified values where provided, otherwise use the estimates
            let ref_time = instantiate_exec
                .args()
                .gas_limit()
                .unwrap_or_else(|| instantiate_result.gas_required.ref_time());
            let proof_size = instantiate_exec
                .args()
                .proof_size()
                .unwrap_or_else(|| instantiate_result.gas_required.proof_size());
            Ok(Weight::from_parts(ref_time, proof_size))
        }
        Err(ref err) => {
            let object = ErrorVariant::from_dispatch_error(
                err,
                &instantiate_exec.client().metadata(),
            )?;
            if instantiate_exec.output_json() {
                Err(anyhow!("{}", serde_json::to_string_pretty(&object)?))
            } else {
                name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(
                    &instantiate_result,
                )?;

                Err(anyhow!("Pre-submission dry-run failed. Use --skip-dry-run to skip this step."))
            }
        }
    }
}

/// A helper function to estimate the gas required for a contract call.
pub async fn pre_submit_dry_run_gas_estimate_call(
    call_exec: &CallExec,
) -> Result<Weight> {
    if call_exec.opts().skip_dry_run() {
        return match (call_exec.gas_limit(), call_exec.proof_size()) {
            (Some(ref_time), Some(proof_size)) => Ok(Weight::from_parts(ref_time, proof_size)),
            _ => {
                Err(anyhow!(
                "Weight args `--gas` and `--proof-size` required if `--skip-dry-run` specified"
            ))
            }
        };
    }
    if !call_exec.output_json() {
        print_dry_running_status(call_exec.message());
    }
    let call_result = call_exec.call_dry_run().await?;
    match call_result.result {
        Ok(_) => {
            if !call_exec.output_json() {
                print_gas_required_success(call_result.gas_required);
            }
            // use user specified values where provided, otherwise use the estimates
            let ref_time = call_exec
                .gas_limit()
                .unwrap_or_else(|| call_result.gas_required.ref_time());
            let proof_size = call_exec
                .proof_size()
                .unwrap_or_else(|| call_result.gas_required.proof_size());
            Ok(Weight::from_parts(ref_time, proof_size))
        }
        Err(ref err) => {
            let object =
                ErrorVariant::from_dispatch_error(err, &call_exec.client().metadata())?;
            if call_exec.output_json() {
                Err(anyhow!("{}", serde_json::to_string_pretty(&object)?))
            } else {
                name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                display_contract_exec_result::<_, MAX_KEY_COL_WIDTH>(&call_result)?;

                Err(anyhow!("Pre-submission dry-run failed. Use --skip-dry-run to skip this step."))
            }
        }
    }
}

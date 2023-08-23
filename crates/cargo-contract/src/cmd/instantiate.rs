use crate::{
    anyhow,
    print_dry_running_status,
    print_gas_required_success,
    ErrorVariant,
    InstantiateExec,
    Weight,
};
use anyhow::Result;
use contract_build::{
    name_value_println,
    util::{
        decode_hex,
        DEFAULT_KEY_COL_WIDTH,
    },
};
use contract_extrinsics::{
    display_contract_exec_result,
    display_contract_exec_result_debug,
    display_dry_run_result_warning,
    prompt_confirm_tx,
    BalanceVariant,
    Code,
    ExtrinsicOpts,
    InstantiateCommandBuilder,
    MAX_KEY_COL_WIDTH,
};
use sp_core::Bytes;
use std::fmt::Debug;
use tokio::runtime::Runtime;

#[derive(Debug, clap::Args)]
pub struct InstantiateCommand {
    /// The name of the contract constructor to call
    #[clap(name = "constructor", long, default_value = "new")]
    constructor: String,
    /// The constructor arguments, encoded as strings
    #[clap(long, num_args = 0..)]
    args: Vec<String>,
    #[clap(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Transfers an initial balance to the instantiated contract
    #[clap(name = "value", long, default_value = "0")]
    value: BalanceVariant,
    /// Maximum amount of gas to be used for this command.
    /// If not specified will perform a dry-run to estimate the gas consumed for the
    /// instantiation.
    #[clap(name = "gas", long)]
    gas_limit: Option<u64>,
    /// Maximum proof size for this instantiation.
    /// If not specified will perform a dry-run to estimate the proof size required.
    #[clap(long)]
    proof_size: Option<u64>,
    /// A salt used in the address derivation of the new contract. Use to create multiple
    /// instances of the same contract code from the same account.
    #[clap(long, value_parser = parse_hex_bytes)]
    salt: Option<Bytes>,
    /// Export the instantiate output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

/// Parse hex encoded bytes.
fn parse_hex_bytes(input: &str) -> Result<Bytes> {
    let bytes = decode_hex(input)?;
    Ok(bytes.into())
}

impl InstantiateCommand {
    /// Returns whether to export the call output in JSON format.
    pub fn output_json(&self) -> bool {
        self.output_json
    }
}

pub fn handle_instantiate(
    instantiate_command: &InstantiateCommand,
) -> Result<(), ErrorVariant> {
    Runtime::new()?.block_on(async {
        let instantiate_exec = InstantiateCommandBuilder::default()
            .constructor(instantiate_command.constructor.clone())
            .args(instantiate_command.args.clone())
            .extrinsic_opts(instantiate_command.extrinsic_opts.clone())
            .value(instantiate_command.value.clone())
            .gas_limit(instantiate_command.gas_limit)
            .proof_size(instantiate_command.proof_size)
            .salt(instantiate_command.salt.clone())
            .output_json(instantiate_command.output_json)
            .done()
            .await;

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

/// A helper function to estimate the gas required for a contract instantiation.
async fn pre_submit_dry_run_gas_estimate_instantiate(
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

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
    call_with_config,
    ErrorVariant,
};

use contract_build::util::DEFAULT_KEY_COL_WIDTH;
use ink_env::Environment;
use serde::Serialize;
use std::{
    fmt::{
        Debug,
        Display,
    },
    str::FromStr,
};

use super::{
    config::SignerConfig,
    display_contract_exec_result,
    display_dry_run_result_warning,
    offer_map_account_if_needed,
    parse_account,
    parse_balance,
    print_dry_running_status,
    print_gas_required_success,
    prompt_confirm_tx,
    CLIExtrinsicOpts,
    MAX_KEY_COL_WIDTH,
};
use anyhow::{
    anyhow,
    Context,
    Result,
};
use contract_build::name_value_println;
use contract_extrinsics::{
    fetch_contract_info,
    pallet_revive_primitives::StorageDeposit,
    CallCommandBuilder,
    CallExec,
    DisplayEvents,
    ExtrinsicOptsBuilder,
    TokenMetadata,
};
use contract_transcode::Value;
use num_traits::Zero;
use sp_core::Decode;
use sp_weights::Weight;
use subxt::{
    config::{
        DefaultExtrinsicParams,
        ExtrinsicParams,
        HashFor,
    },
    ext::{
        scale_decode::IntoVisitor,
        scale_encode::EncodeAsType,
    },
    Config,
};

#[derive(Debug, clap::Args)]
#[clap(name = "call", about = "Call a contract")]
pub struct CallCommand {
    /// The address of the contract to call.
    #[clap(name = "contract", long, env = "CONTRACT")]
    contract: String,
    /// The name of the contract message to call.
    #[clap(long, short)]
    message: String,
    /// The arguments of the contract message to call.
    #[clap(long, num_args = 0..)]
    args: Vec<String>,
    #[clap(flatten)]
    extrinsic_cli_opts: CLIExtrinsicOpts,
    /// Maximum amount of gas (execution time) to be used for this command.
    /// If not specified will perform a dry-run to estimate the gas consumed for the
    /// call.
    #[clap(name = "gas", long)]
    gas_limit: Option<u64>,
    /// Maximum proof size for this call.
    /// If not specified will perform a dry-run to estimate the proof size required for
    /// the call.
    #[clap(long)]
    proof_size: Option<u64>,
    /// The value to be transferred as part of the call.
    #[clap(name = "value", long, default_value = "0")]
    value: String,
    /// Export the call output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

impl CallCommand {
    /// Returns whether to export the call output in JSON format.
    pub fn output_json(&self) -> bool {
        self.output_json
    }

    pub async fn handle(&self) -> Result<(), ErrorVariant> {
        call_with_config!(
            self,
            run,
            self.extrinsic_cli_opts.chain_cli_opts.chain().config()
        )
    }

    async fn run<C: Config + Environment + SignerConfig<C>>(
        &self,
    ) -> Result<(), ErrorVariant>
    where
        <C as Config>::AccountId:
            IntoVisitor + FromStr + EncodeAsType + Decode + AsRef<[u8]> + Display,
        <<C as Config>::AccountId as FromStr>::Err: Display,
        C::Balance: From<u128>
            + Display
            + Default
            + FromStr
            + Serialize
            + Debug
            + EncodeAsType
            + IntoVisitor
            + Zero,
        <C::ExtrinsicParams as ExtrinsicParams<C>>::Params:
            From<<DefaultExtrinsicParams<C> as ExtrinsicParams<C>>::Params>,
        HashFor<C>: IntoVisitor,
    {
        let contract = parse_account(&self.contract)
            .map_err(|e| anyhow::anyhow!("Failed to parse contract option: {}", e))?;
        let signer = C::Signer::from_str(&self.extrinsic_cli_opts.suri)
            .map_err(|_| anyhow::anyhow!("Failed to parse suri option"))?;
        let chain = self.extrinsic_cli_opts.chain_cli_opts.chain();
        let token_metadata = TokenMetadata::query::<C>(&chain.url()).await?;

        let extrinsic_opts = ExtrinsicOptsBuilder::new(signer.clone())
            .file(self.extrinsic_cli_opts.file.clone())
            .manifest_path(self.extrinsic_cli_opts.manifest_path.clone())
            .url(chain.url())
            .verbosity(self.extrinsic_cli_opts.verbosity()?)
            .done();
        offer_map_account_if_needed(extrinsic_opts).await?;

        let storage_deposit_limit = self
            .extrinsic_cli_opts
            .storage_deposit_limit
            .clone()
            .map(|b| parse_balance(&b, &token_metadata))
            .transpose()
            .map_err(|e| {
                anyhow::anyhow!("Failed to parse storage_deposit_limit option: {}", e)
            })?;
        let value = parse_balance(&self.value, &token_metadata)
            .map_err(|e| anyhow::anyhow!("Failed to parse value option: {}", e))?;
        let extrinsic_opts = ExtrinsicOptsBuilder::new(signer)
            .file(self.extrinsic_cli_opts.file.clone())
            .manifest_path(self.extrinsic_cli_opts.manifest_path.clone())
            .url(chain.url())
            .storage_deposit_limit(storage_deposit_limit)
            .verbosity(self.extrinsic_cli_opts.verbosity()?)
            .done();

        let call_exec = CallCommandBuilder::new(contract, &self.message, extrinsic_opts)
            .args(self.args.clone())
            .gas_limit(self.gas_limit)
            .proof_size(self.proof_size)
            .value(value)
            .done()
            .await?;

        // assert that the contract even exists
        let _ =
            fetch_contract_info::<C, C>(&contract, call_exec.rpc(), call_exec.client())
                .await?;

        let metadata = call_exec.client().metadata();

        if !self.extrinsic_cli_opts.execute {
            let result = call_exec.call_dry_run().await?;
            match result.result {
                Ok(ref ret_val) => {
                    if ret_val.did_revert() {
                        let data = ret_val.data[1..].to_vec();
                        let msg = String::from_utf8(data).unwrap();
                        panic!("Call did revert {msg:?}");
                        /*
                        // todo
                        ErrorVariant::
                        let object = ErrorVariant::from_dispatch_error(err, &metadata)?;
                        if self.output_json() {
                            return Err(object)
                        } else {
                            name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                            display_contract_exec_result::<_, MAX_KEY_COL_WIDTH, _>(&result)?;
                        }
                        return
                        */
                    }
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
                        storage_deposit: result.storage_deposit.clone(),
                    };
                    if self.output_json() {
                        println!("{}", dry_run_result.to_json()?);
                    } else {
                        dry_run_result.print();
                        display_dry_run_result_warning("message");
                    };
                }
                Err(ref err) => {
                    let object = ErrorVariant::from_dispatch_error(err, &metadata)?;
                    if self.output_json() {
                        return Err(object)
                    } else {
                        name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                        display_contract_exec_result::<_, MAX_KEY_COL_WIDTH, _>(&result)?;
                    }
                }
            }
        } else {
            let (gas_limit, storage_deposit_limit) =
                pre_submit_dry_run_gas_estimate_call(
                    &call_exec,
                    self.output_json(),
                    self.extrinsic_cli_opts.skip_dry_run,
                )
                .await?;
            if !self.extrinsic_cli_opts.skip_confirm {
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
            let events = call_exec
                .call(Some(gas_limit), Some(storage_deposit_limit))
                .await?;
            let display_events =
                DisplayEvents::from_events::<C, C>(&events, None, &metadata)?;

            let output = if self.output_json() {
                display_events.to_json()?
            } else {
                display_events.display_events::<C>(
                    self.extrinsic_cli_opts.verbosity().unwrap(),
                    &token_metadata,
                )?
            };
            println!("{output}");
        }
        Ok(())
    }
}

/// A helper function to estimate the gas required for a contract call.
async fn pre_submit_dry_run_gas_estimate_call<C: Config + Environment, Signer>(
    call_exec: &CallExec<C, C, Signer>,
    output_json: bool,
    skip_dry_run: bool,
) -> Result<(Weight, C::Balance)>
where
    Signer: subxt::tx::Signer<C> + Clone,
    <C as Config>::AccountId: IntoVisitor + EncodeAsType,
    C::Balance: Debug + EncodeAsType + Zero,
    <C::ExtrinsicParams as ExtrinsicParams<C>>::Params:
        From<<DefaultExtrinsicParams<C> as ExtrinsicParams<C>>::Params>,
{
    if skip_dry_run {
        let weight = match (call_exec.gas_limit(), call_exec.proof_size()) {
            (Some(ref_time), Some(proof_size)) => Ok(Weight::from_parts(ref_time, proof_size)),
            _ => {
                Err(anyhow!(
                "Weight args `--gas` and `--proof-size` required if `--skip-dry-run` specified"
            ))
            }
        }?;
        let storage_deposit_limit = match call_exec.opts().storage_deposit_limit() {
            Some(limit) => Ok(limit),
            _ => {
                Err(anyhow!(
                        "Storage deposit limit arg `--storage-deposit-limit` required if `--skip-dry-run` specified"
                    ))
            }
        }?;
        return Ok((weight, storage_deposit_limit));
    }
    if !output_json {
        print_dry_running_status(call_exec.message());
    }
    let call_result = call_exec.call_dry_run().await?;
    match call_result.result {
        Ok(_) => {
            if !output_json {
                print_gas_required_success(call_result.gas_required);
            }
            // use user specified values where provided, otherwise use the estimates
            let ref_time = call_exec
                .gas_limit()
                .unwrap_or_else(|| call_result.gas_required.ref_time());
            let proof_size = call_exec
                .proof_size()
                .unwrap_or_else(|| call_result.gas_required.proof_size());
            let storage_deposit_limit =
                call_exec.opts().storage_deposit_limit().unwrap_or_else(|| {
                    match call_result.storage_deposit {
                        StorageDeposit::Refund(_) => C::Balance::zero(),
                        StorageDeposit::Charge(charge) => charge,
                    }
                });
            Ok((
                Weight::from_parts(ref_time, proof_size),
                storage_deposit_limit,
            ))
        }
        Err(ref err) => {
            let object =
                ErrorVariant::from_dispatch_error(err, &call_exec.client().metadata())?;
            if output_json {
                Err(anyhow!("{}", serde_json::to_string_pretty(&object)?))
            } else {
                name_value_println!("Result", object, MAX_KEY_COL_WIDTH);
                display_contract_exec_result::<_, MAX_KEY_COL_WIDTH, _>(&call_result)?;

                Err(anyhow!("Pre-submission dry-run failed. Use --skip-dry-run to skip this step."))
            }
        }
    }
}

/// Result of the contract call
#[derive(serde::Serialize)]
pub struct CallDryRunResult<Balance> {
    /// Was the operation reverted
    pub reverted: bool,
    pub data: Value,
    pub gas_consumed: Weight,
    pub gas_required: Weight,
    /// Storage deposit after the operation
    pub storage_deposit: StorageDeposit<Balance>,
}

impl<Balance: Serialize> CallDryRunResult<Balance> {
    /// Returns a result in json format
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn print(&self) {
        name_value_println!("Result", format!("{}", self.data), DEFAULT_KEY_COL_WIDTH);
        name_value_println!(
            "Reverted",
            format!("{:?}", self.reverted),
            DEFAULT_KEY_COL_WIDTH
        );
    }
}

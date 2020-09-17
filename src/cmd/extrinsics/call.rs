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

use std::fs::File;

use crate::{crate_metadata::CrateMetadata, workspace::ManifestPath, ExtrinsicOpts};
use anyhow::Result;
use ink_metadata::InkProject;
use structopt::StructOpt;
use subxt::{
    balances::Balances, contracts::*, system::System, ClientBuilder, ContractsTemplateRuntime,
};

#[derive(Debug, StructOpt)]
#[structopt(name = "call", about = "Call a contract")]
pub struct CallCommand {
    #[structopt(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Maximum amount of gas to be used for this command
    #[structopt(name = "gas", long, default_value = "500000000")]
    gas_limit: u64,
    /// The value to be transferred as part of the call
    value: <ContractsTemplateRuntime as Balances>::Balance,
    /// The address of the the contract to call
    contract: <ContractsTemplateRuntime as System>::AccountId,
    /// The name of the contract message to call
    name: String,
    /// The call arguments, encoded as strings
    args: Vec<String>,
}

impl CallCommand {
    pub fn run(&self) -> Result<String> {
        let metadata = super::load_metadata()?;
        let msg_encoder = super::MessageEncoder::new(metadata);
        let call_data = msg_encoder.encode_message(&self.name, &self.args)?;

        async_std::task::block_on(async move {
            let cli = ClientBuilder::<ContractsTemplateRuntime>::new()
                .set_url(&self.extrinsic_opts.url.to_string())
                .build()
                .await?;
            let signer = self.extrinsic_opts.signer()?;

            let events = cli
                .call_and_watch(
                    &signer,
                    &self.contract,
                    self.value,
                    self.gas_limit,
                    &call_data,
                )
                .await?;
            let executed = events
                .contract_execution()?
                .ok_or(anyhow::anyhow!("Failed to find ContractExecution event"))?;

            // todo: decode executed data (events)
            Ok(hex::encode(executed.data))
        })
    }
}

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

use std::{
	fs::File,
};

use anyhow::Result;
use ink_metadata::InkProject;
use structopt::StructOpt;
use subxt::{balances::Balances, contracts::*, system::System, ClientBuilder, ContractsTemplateRuntime};
use crate::{
	ExtrinsicOpts,
	crate_metadata::CrateMetadata,
	workspace::ManifestPath,
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
		let manifest_path = ManifestPath::default();
		// todo: add metadata path option
		let metadata_path: Option<std::path::PathBuf> = None;
		let path = match metadata_path {
			Some(path) => path,
			None => {
				let crate_metadata = CrateMetadata::collect(&manifest_path)?;
				crate_metadata.metadata_path()
			}
		};
		let metadata: InkProject = serde_json::from_reader(File::open(path)?)?;

		let calls = metadata.spec.messages.iter().map(|m| m.name.clone()).collect::<Vec<_>>();

		let msg =
			metadata.spec.messages
				.iter()
				.find(|msg| msg.name == self.name)
				.ok_or(anyhow::anyhow!("A contract call named '{}' was not found. Expected one of {:?}", self.name, calls))?;

		let call_data = msg.encode_message(&self.args)?;

		async_std::task::block_on(async move {
			let cli = ClientBuilder::<ContractsTemplateRuntime>::new()
				.set_url(&self.extrinsic_opts.url.to_string())
				.build()
				.await?;
			let signer = self.extrinsic_opts.signer()?;

			let events = cli
				.call_and_watch(&signer, &self.contract, self.value, self.gas_limit, &call_data)
				.await?;
			let executed = events
				.contract_execution()?
				.ok_or(anyhow::anyhow!("Failed to find ContractExecution event"))?;

			// todo: decode executed data (events)
			Ok(hex::encode(executed.data))
		})
	}
}

use ink_metadata::MessageSpec;
use scale_info::form::CompactForm;

pub trait EncodeMessage {
	// todo: rename
	fn encode_message(&self, args: &[String])-> Result<Vec<u8>>;
}

impl EncodeMessage for MessageSpec<CompactForm> {
	fn encode_message(&self, _args: &[String]) -> Result<Vec<u8>> {
		Ok(self.selector.to_vec())
	}
}


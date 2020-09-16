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
	path::Path,
};

use anyhow::Result;
use ink_metadata::InkProject;
// use subxt::{balances::Balances, contracts::*, system::System, ClientBuilder, DefaultNodeRuntime};
// use crate::{ExtrinsicOpts, HexData};
use crate::{
	crate_metadata::CrateMetadata,
	workspace::ManifestPath,
};

pub(crate) fn list<P>(
	manifest_path: ManifestPath,
	metadata_path: Option<P>
) -> Result<Vec<String>>
where
	P: AsRef<Path>
{
	let path = match metadata_path {
		Some(path) => path.as_ref().to_path_buf(),
		None => {
			let crate_metadata = CrateMetadata::collect(&manifest_path)?;
			crate_metadata.metadata_path()
		}
	};

	let metadata: InkProject = serde_json::from_reader(File::open(path)?)?;
	let calls = metadata.spec.messages
		.iter()
		.map(|msg| {
			msg.name.clone()
		})
		.collect();
	Ok(calls)
}

// /// Instantiate a contract stored at the supplied code hash.
// /// Returns the account id of the instantiated contract if successful.
// ///
// /// Creates an extrinsic with the `Contracts::instantiate` Call, submits via RPC, then waits for
// /// the `ContractsEvent::Instantiated` event.
// pub(crate) fn execute_call(
// 	extrinsic_opts: &ExtrinsicOpts,
// 	endowment: <DefaultNodeRuntime as Balances>::Balance,
// 	gas_limit: u64,
// 	code_hash: <DefaultNodeRuntime as System>::Hash,
// 	data: HexData,
// ) -> Result<<DefaultNodeRuntime as System>::AccountId> {
// 	todo!()
// 	// async_std::task::block_on(async move {
// 	// 	let cli = ClientBuilder::<DefaultNodeRuntime>::new()
// 	// 		.set_url(&extrinsic_opts.url.to_string())
// 	// 		.build()
// 	// 		.await?;
// 	// 	let signer = extrinsic_opts.signer()?;
// 	//
// 	// 	let events = cli
// 	// 		.instantiate_and_watch(&signer, endowment, gas_limit, &code_hash, &data.0)
// 	// 		.await?;
// 	// 	let instantiated = events
// 	// 		.instantiated()?
// 	// 		.ok_or(anyhow::anyhow!("Failed to find Instantiated event"))?;
// 	//
// 	// 	Ok(instantiated.contract)
// 	// })
// }

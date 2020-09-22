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

pub mod deploy;
pub mod instantiate;
pub mod call;

use anyhow::Result;
use codec::Encode as _;
use ink_metadata::{
	InkProject,
	MessageParamSpec,
	Selector,
};
use scale_info::{
	form::{CompactForm, Form}, Type, TypeDef, TypeDefArray, TypeDefComposite, TypeDefPrimitive,
	TypeDefSequence, TypeDefTuple, TypeDefVariant,
};
use std::{
	fs::File,
	str::FromStr,
};
use crate::{crate_metadata::CrateMetadata, workspace::ManifestPath};

pub fn load_metadata() -> Result<InkProject> {
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
	let metadata = serde_json::from_reader(File::open(path)?)?;
	Ok(metadata)
}

struct MessageEncoder {
	metadata: InkProject
}

impl MessageEncoder {
	pub fn new(metadata: InkProject) -> Self {
		Self {
			metadata
		}
	}

	fn encode_constructor<I, S>(&self, name: &str, args: I) -> Result<Vec<u8>>
	where
		I: IntoIterator<Item = S>,
		S: AsRef<str>,
	{
		let constructors = self.metadata
			.spec
			.constructors
			.iter()
			.map(|m| m.name.clone())
			.collect::<Vec<_>>();

		let constructor_spec = self
			.metadata
			.spec
			.constructors
			.iter()
			.find(|msg| msg.name == name)
			.ok_or(anyhow::anyhow!(
                "A contract call named '{}' was not found. Expected one of {:?}",
                name,
                constructors
            ))?;

		self.encode(&constructor_spec.selector, &constructor_spec.args, args)

	}

	fn encode_message<I, S>(&self, name: &str, args: I) -> Result<Vec<u8>>
	where
		I: IntoIterator<Item = S>,
		S: AsRef<str>,
	{
		let calls = self.metadata
			.spec
			.messages
			.iter()
			.map(|m| m.name.clone())
			.collect::<Vec<_>>();

		let msg_spec = self
			.metadata
			.spec
			.messages
			.iter()
			.find(|msg| msg.name == name)
			.ok_or(anyhow::anyhow!(
                "A contract call named '{}' was not found. Expected one of {:?}",
                name,
                calls
            ))?;

		self.encode(&msg_spec.selector, &msg_spec.args, args)
	}

	fn encode<I, S>(&self, spec_selector: &Selector, spec_args: &[MessageParamSpec<CompactForm>], args: I) -> Result<Vec<u8>>
		where
			I: IntoIterator<Item = S>,
			S: AsRef<str>,
	{
		let mut args = spec_args
			.iter()
			.zip(args)
			.map(|(spec, arg)| {
				let ty = self.metadata
					.registry
					.resolve(spec.ty.id.id)
					.ok_or(anyhow::anyhow!(
                    "Failed to resolve type for arg '{:?}' with id '{}'",
                    spec.name,
                    spec.ty.id.id
                ))?;
				ty.type_def.encode_arg(arg.as_ref())
			})
			.collect::<Result<Vec<_>>>()?
			.concat();
		let mut encoded = spec_selector.to_vec();
		encoded.append(&mut args);
		Ok(encoded)
	}
}

pub trait EncodeContractArg {
	// todo: rename
	fn encode_arg(&self, arg: &str) -> Result<Vec<u8>>;
}

impl EncodeContractArg for TypeDef<CompactForm> {
	fn encode_arg(&self, arg: &str) -> Result<Vec<u8>> {
		match self {
			TypeDef::Primitive(primitive) => primitive.encode_arg(arg),
			_ => unimplemented!(),
		}
	}
}

impl EncodeContractArg for TypeDefPrimitive {
	fn encode_arg(&self, arg: &str) -> Result<Vec<u8>> {
		match self {
			TypeDefPrimitive::Bool => Ok(bool::encode(&bool::from_str(arg)?)),
			TypeDefPrimitive::I32 => Ok(i32::encode(&i32::from_str(arg)?)),
			_ => unimplemented!(),
		}
	}
}

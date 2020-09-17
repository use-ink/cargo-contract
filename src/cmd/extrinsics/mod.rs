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
	MessageSpec,
};
use scale_info::{
	form::CompactForm, Type, TypeDef, TypeDefArray, TypeDefComposite, TypeDefPrimitive,
	TypeDefSequence, TypeDefTuple, TypeDefVariant,
};
use std::str::FromStr;

fn encode_message<I, S>(
	ink_project: &InkProject,
	msg: &MessageSpec<CompactForm>,
	args: I,
) -> Result<Vec<u8>>
	where
		I: IntoIterator<Item = S>,
		S: AsRef<str>,
{
	let mut args = msg
		.args
		.iter()
		.zip(args)
		.map(|(spec, arg)| {
			let ty = ink_project
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
	let mut encoded = msg.selector.to_vec();
	encoded.append(&mut args);
	Ok(encoded)
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
			TypeDefPrimitive::I32 => Ok(i32::encode(&i32::from_str(arg)?)),
			_ => unimplemented!(),
		}
	}
}

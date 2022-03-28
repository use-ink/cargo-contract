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

use super::load_metadata;
use crate::cmd::extrinsics::{ExtrinsicOpts, ContractMessageTranscoder};
use anyhow::Result;


#[derive(Debug, clap::Args)]
#[clap(name = "decode", about = "Decode input_data for a contract")]
pub struct DecodeCommand {
    #[clap(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// The data to decode
    #[clap(long)]
    data: String,
}

impl DecodeCommand {
    pub fn run(&self) -> Result<()> {
        let (_, contract_metadata) = load_metadata(self.extrinsic_opts.manifest_path.as_ref())?;
        let transcoder = ContractMessageTranscoder::new(&contract_metadata);
        let decoded_data = transcoder.decode_contract_event(&mut &self.data.as_bytes()[..]);

	log::debug!("DECODED DATA: {:?}", decoded_data);
	Ok(())
    }
}

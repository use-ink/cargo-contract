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

use crate::{
    cmd::extrinsics::{
        load_metadata,
        ContractMessageTranscoder,
    },
    util::decode_hex,
    DEFAULT_KEY_COL_WIDTH,
};
use anyhow::{
    Context,
    Result,
};
use colored::Colorize as _;

#[derive(Debug, Clone, clap::Args)]
#[clap(name = "decode", about = "Decode input_data for a contract")]
pub struct DecodeCommand {
    /// Type of data
    #[clap(arg_enum, short, long)]
    r#type: DataType,
    /// The data to decode
    #[clap(short, long)]
    data: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ArgEnum)]
enum DataType {
    Event,
    Message,
    Constructor,
}

impl DecodeCommand {
    pub fn run(&self) -> Result<()> {
        let (_, contract_metadata) = load_metadata(None)?;
        let transcoder = ContractMessageTranscoder::new(&contract_metadata);

        const ERR_MSG: &str = "Failed to decode specified data as a hex value";
        let decoded_data = match self.r#type {
            DataType::Event => {
                transcoder.decode_contract_event(
                    &mut &decode_hex(&self.data).context(ERR_MSG)?[..],
                )?
            }
            DataType::Message => {
                transcoder.decode_contract_message(
                    &mut &decode_hex(&self.data).context(ERR_MSG)?[..],
                )?
            }
            DataType::Constructor => {
                transcoder.decode_contract_constructor(
                    &mut &decode_hex(&self.data).context(ERR_MSG)?[..],
                )?
            }
        };

        println!(
            "{:>width$} {}",
            "Decoded data:".bright_green().bold(),
            decoded_data,
            width = DEFAULT_KEY_COL_WIDTH
        );

        Ok(())
    }
}

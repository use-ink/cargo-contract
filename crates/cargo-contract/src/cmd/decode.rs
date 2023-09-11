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

use crate::DEFAULT_KEY_COL_WIDTH;
use anyhow::{
    Context,
    Result,
};
use clap::{
    Args,
    Subcommand,
};
use colored::Colorize as _;
use contract_build::{
    util,
    CrateMetadata,
};
use contract_transcode::ContractMessageTranscoder;

#[derive(Debug, Args)]
pub struct DecodeCommand {
    #[clap(subcommand)]
    commands: DecodeCommands,
}

#[derive(Debug, Subcommand)]
pub enum DecodeCommands {
    #[clap(name = "message")]
    Message(DecodeMessage),
    /// Upload contract code
    #[clap(name = "constructor")]
    Constructor(DecodeConstructor),
    /// Instantiate a contract
    #[clap(name = "event")]
    Event(DecodeEvent),
}

#[derive(Debug, Clone, Args)]
pub struct DecodeMessage {
    /// The data to decode; this has to be a hex value starting with `0x`.
    #[clap(short, long)]
    data: String,
}

#[derive(Debug, Clone, Args)]
pub struct DecodeConstructor {
    /// The data to decode; this has to be a hex value starting with `0x`.
    #[clap(short, long)]
    data: String,
}

#[derive(Debug, Clone, Args)]
pub struct DecodeEvent {
    /// The signature topic of the event to be decoded; this has to be a hex value
    /// starting with `0x`.
    #[clap(short, long)]
    signature_topic: String,
    /// The data to decode; this has to be a hex value starting with `0x`.
    #[clap(short, long)]
    data: String,
}

impl DecodeCommand {
    pub fn run(&self) -> Result<()> {
        let crate_metadata =
            CrateMetadata::from_manifest_path(None, contract_build::Target::Wasm)?;
        let transcoder = ContractMessageTranscoder::load(crate_metadata.metadata_path())?;

        const ERR_MSG: &str = "Failed to decode specified data as a hex value";
        let decoded_data = match &self.commands {
            DecodeCommands::Event(event) => {
                let signature_topic_data =
                    util::decode_hex(&event.signature_topic).context(ERR_MSG)?;
                let signature_topic =
                    primitive_types::H256::from_slice(&signature_topic_data);
                transcoder.decode_contract_event(
                    &signature_topic,
                    &mut &util::decode_hex(&event.data).context(ERR_MSG)?[..],
                )?
            }
            DecodeCommands::Message(message) => {
                transcoder.decode_contract_message(
                    &mut &util::decode_hex(&message.data).context(ERR_MSG)?[..],
                )?
            }
            DecodeCommands::Constructor(constructor) => {
                transcoder.decode_contract_constructor(
                    &mut &util::decode_hex(&constructor.data).context(ERR_MSG)?[..],
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

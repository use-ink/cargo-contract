// Copyright 2018-2023 Parity Technologies (UK) Ltd.
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

use super::{
    basic_display_format_contract_info,
    display_all_contracts,
    DefaultConfig,
};
use anyhow::{
    anyhow,
    Result,
};
use contract_extrinsics::{
    fetch_all_contracts,
    fetch_contract_info,
    fetch_wasm_code,
    ErrorVariant,
};
use std::{
    fmt::Debug,
    io::Write,
};
use subxt::{
    Config,
    OnlineClient,
};
use tokio::runtime::Runtime;

#[derive(Debug, clap::Args)]
#[clap(name = "info", about = "Get infos from a contract")]
pub struct InfoCommand {
    /// The address of the contract to display info of.
    #[clap(
        name = "contract",
        long,
        env = "CONTRACT",
        required_unless_present = "all"
    )]
    contract: Option<<DefaultConfig as Config>::AccountId>,
    /// Websockets url of a substrate node.
    #[clap(
        name = "url",
        long,
        value_parser,
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
    /// Export the instantiate output in JSON format.
    #[clap(name = "output-json", long)]
    output_json: bool,
    /// Display the contract's Wasm bytecode.
    #[clap(name = "binary", long, conflicts_with = "all")]
    binary: bool,
    /// Display all contracts addresses
    #[clap(name = "all", long)]
    all: bool,
}

impl InfoCommand {
    pub fn run(&self) -> Result<(), ErrorVariant> {
        Runtime::new()?.block_on(async {
            let url = self.url.clone();
            let client = OnlineClient::<DefaultConfig>::from_url(url).await?;

            // All flag applied
            if self.all {
                // Using count of 255 to avoid conversion issue to usize on
                // any architecture
                let count = 255;
                let mut from = None;
                let mut contracts = Vec::new();
                loop {
                    let len = contracts.len();
                    contracts
                        .append(&mut fetch_all_contracts(&client, count, from).await?);
                    if contracts.len()
                        < len
                            + <u32 as TryInto<usize>>::try_into(count)
                                .expect("Conversion from u32 to usize failed")
                    {
                        break
                    }
                    from = contracts.last();
                }

                if self.output_json {
                    let contracts_json = serde_json::json!({
                        "contracts": contracts
                    });
                    println!("{}", serde_json::to_string_pretty(&contracts_json)?);
                } else {
                    display_all_contracts(&contracts)
                }
                Ok(())
            } else {
                // Contract arg shall be always present in this case, it is enforced by
                // clap configuration
                let contract = self
                    .contract
                    .as_ref()
                    .expect("Contract argument was not provided");

                let info_to_json =
                    fetch_contract_info(contract, &client)
                        .await?
                        .ok_or(anyhow!(
                            "No contract information was found for account id {}",
                            contract
                        ))?;

                match (self.output_json, self.binary) {
                    (true, false) => println!("{}", info_to_json.to_json()?),
                    (false, false) => basic_display_format_contract_info(&info_to_json),
                    // Binary flag applied
                    (_, true) => {
                        let wasm_code =
                            fetch_wasm_code(*info_to_json.code_hash(), &client).await?;
                        match (wasm_code, self.output_json) {
                            (Some(code), false) => {
                                std::io::stdout()
                                    .write_all(&code)
                                    .expect("Writing to stdout failed")
                            }
                            (Some(code), true) => {
                                let wasm = serde_json::json!({
                                    "wasm": format!("0x{}", hex::encode(code))
                                });
                                println!("{}", serde_json::to_string_pretty(&wasm)?);
                            }
                            (None, _) => {
                                return Err(
                                    anyhow!("Contract wasm code was not found").into()
                                )
                            }
                        }
                    }
                }
                Ok(())
            }
        })
    }
}

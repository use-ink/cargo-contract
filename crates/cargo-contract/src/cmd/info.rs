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

use super::DefaultConfig;
use anyhow::{
    anyhow,
    Result,
};
use contract_extrinsics::{
    fetch_contract_info,
    ErrorVariant,
};
use std::fmt::Debug;
use subxt::{
    Config,
    OnlineClient,
};
use tokio::runtime::Runtime;

#[derive(Debug, clap::Args)]
#[clap(name = "info", about = "Get infos from a contract")]
pub struct InfoCommand {
    /// The address of the contract to display info of.
    #[clap(name = "contract", long, env = "CONTRACT")]
    contract: <DefaultConfig as Config>::AccountId,
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
}

impl InfoCommand {
    pub fn run(&self) -> Result<(), ErrorVariant> {
        tracing::debug!(
            "Getting contract information for AccountId {:?}",
            self.contract
        );

        Runtime::new()?.block_on(async {
            let url = self.url.clone();
            let client = OnlineClient::<DefaultConfig>::from_url(url).await?;

            let info_result = fetch_contract_info(&self.contract, &client).await?;

            match info_result {
                Some(info_to_json) => {
                    if self.output_json {
                        println!("{}", info_to_json.to_json()?);
                    } else {
                        info_to_json.basic_display_format_contract_info();
                    }
                    Ok(())
                }
                None => {
                    Err(anyhow!(
                        "No contract information was found for account id {}",
                        self.contract
                    )
                    .into())
                }
            }
        })
    }
}

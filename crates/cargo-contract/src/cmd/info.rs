// Copyright (C) Use Ink (UK) Ltd.
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

use crate::call_with_config;

use super::{
    basic_display_format_extended_contract_info,
    display_all_contracts,
    parse_addr,
    CLIChainOpts,
};
use anyhow::Result;
use contract_analyze::determine_language;
use contract_extrinsics::{
    fetch_all_contracts,
    fetch_contract_binary,
    fetch_contract_info,
    url_to_string,
    ContractInfo,
    ErrorVariant,
    TrieId,
};
use ink_env::Environment;
use primitive_types::H256;
use serde::Serialize;
use std::{
    fmt::{
        Debug,
        Display,
    },
    io::Write,
    str::FromStr,
};
use subxt::{
    backend::{
        legacy::LegacyRpcMethods,
        rpc::RpcClient,
    },
    ext::{
        codec::Decode,
        scale_decode::IntoVisitor,
    },
    Config,
    OnlineClient,
};

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
    contract: Option<String>,
    /// Export the instantiate output in JSON format.
    #[clap(name = "output-json", long)]
    output_json: bool,
    /// Display the contract's binary.
    #[clap(name = "binary", long, conflicts_with = "all")]
    binary: bool,
    /// Display all contracts addresses
    #[clap(name = "all", long)]
    all: bool,
    /// Arguments required for communicating with a Substrate node.
    #[clap(flatten)]
    chain_cli_opts: CLIChainOpts,
}

impl InfoCommand {
    pub async fn handle(&self) -> Result<(), ErrorVariant> {
        call_with_config!(self, run, self.chain_cli_opts.chain().config())
    }

    pub async fn run<C: Config + Environment>(&self) -> Result<(), ErrorVariant>
    where
        <C as Config>::AccountId:
            Serialize + Display + IntoVisitor + Decode + AsRef<[u8]> + FromStr,
        <C as Config>::Hash: IntoVisitor + Display,
        <C as Environment>::Balance: Serialize + Debug + IntoVisitor,
        <<C as Config>::AccountId as FromStr>::Err:
            Into<Box<(dyn std::error::Error)>> + Display,
    {
        let rpc_cli =
            RpcClient::from_url(url_to_string(&self.chain_cli_opts.chain().url()))
                .await?;
        let client = OnlineClient::<C>::from_rpc_client(rpc_cli.clone()).await?;
        let rpc = LegacyRpcMethods::<C>::new(rpc_cli.clone());

        // All flag applied
        if self.all {
            let contracts = fetch_all_contracts(&client, &rpc).await?;

            if self.output_json {
                let contracts_json = serde_json::json!({
                    "Revive": contracts
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
                .map(|c| parse_addr(c))
                .transpose()?
                .expect("Contract argument shall be present");

            let info_to_json =
                fetch_contract_info::<C, C>(&contract, &rpc, &client).await?;

            let contract_binary =
                fetch_contract_binary(&client, &rpc, info_to_json.code_hash()).await?;

            // Binary flag applied
            if self.binary {
                if self.output_json {
                    let output = serde_json::json!({
                        "contract_binary": format!("0x{}", hex::encode(contract_binary))
                    });
                    println!("{}", serde_json::to_string_pretty(&output)?);
                } else {
                    std::io::stdout()
                        .write_all(&contract_binary)
                        .expect("Writing to stdout failed")
                }
            } else if self.output_json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &ExtendedContractInfo::<C::Balance>::new(
                            info_to_json,
                            &contract_binary
                        )
                    )?
                )
            } else {
                basic_display_format_extended_contract_info(&ExtendedContractInfo::<
                    C::Balance,
                >::new(
                    info_to_json,
                    &contract_binary,
                ))
            }
            Ok(())
        }
    }
}

#[derive(serde::Serialize)]
pub struct ExtendedContractInfo<Balance> {
    pub trie_id: TrieId,
    pub code_hash: H256,
    pub storage_items: u32,
    pub storage_items_deposit: Balance,
    pub storage_total_deposit: Balance,
    pub source_language: String,
}

impl<Balance> ExtendedContractInfo<Balance>
where
    Balance: serde::Serialize + Copy,
{
    pub fn new(contract_info: ContractInfo<Balance>, code: &[u8]) -> Self {
        let language = match determine_language(code).ok() {
            Some(lang) => lang.to_string(),
            None => "Unknown".to_string(),
        };
        ExtendedContractInfo {
            trie_id: contract_info.trie_id().clone(),
            code_hash: *contract_info.code_hash(),
            storage_items: contract_info.storage_items(),
            storage_items_deposit: contract_info.storage_items_deposit(),
            storage_total_deposit: contract_info.storage_total_deposit(),
            source_language: language,
        }
    }
}

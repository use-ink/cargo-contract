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
    basic_display_format_extended_contract_info,
    display_all_contracts,
    DefaultConfig,
};
use anyhow::{
    anyhow,
    Result,
};
use contract_extrinsics::{url_to_string, Balance, CodeHash, ContractInfo, ErrorVariant, ContractArtifacts, fetch_contract_info};
use std::{
    fmt::Debug,
    io::Write,
    path::PathBuf,
};
use subxt::{
    backend::{
        legacy::LegacyRpcMethods,
        rpc::RpcClient,
    },
    Config,
    OnlineClient,
};

#[derive(Debug, clap::Args)]
#[clap(name = "storage", about = "Inspect contract storage")]
pub struct StorageCommand {
    /// The address of the contract to inspect storage of.
    #[clap(
        name = "contract",
        long,
        env = "CONTRACT",
        required_unless_present = "all"
    )]
    contract: <DefaultConfig as Config>::AccountId,
    /// Path to a contract build artifact file: a raw `.wasm` file, a `.contract` bundle,
    /// or a `.json` metadata file.
    #[clap(value_parser, conflicts_with = "manifest_path")]
    file: Option<PathBuf>,
    /// Path to the `Cargo.toml` of the contract.
    #[clap(long, value_parser)]
    manifest_path: Option<PathBuf>,
    /// Websockets url of a substrate node.
    #[clap(
        name = "url",
        long,
        value_parser,
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
    /// Export the storage output in JSON format.
    #[clap(name = "output-json", long)]
    output_json: bool,
}

impl StorageCommand {
    pub async fn run(&self) -> Result<(), ErrorVariant> {
        let rpc_cli = RpcClient::from_url(url_to_string(&self.url)).await?;
        let client =
            OnlineClient::<DefaultConfig>::from_rpc_client(rpc_cli.clone()).await?;
        let rpc = LegacyRpcMethods::<DefaultConfig>::new(rpc_cli.clone());

        let contract_artifacts = ContractArtifacts::from_manifest_or_file(
            self.manifest_path.as_ref(),
            self.file.as_ref(),
        )?;

        let contract_info = fetch_contract_info(&self.contract, &rpc, &client).await?
            .ok_or(anyhow!(
                "No contract information was found for account id {}",
                contract
            ))?;

        Ok(())
    }
}

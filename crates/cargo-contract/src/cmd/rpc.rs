// Copyright 2018-2024 Parity Technologies (UK) Ltd.
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

use contract_extrinsics::{
    ErrorVariant,
    RawParams,
    RpcRequest,
};

#[derive(Debug, clap::Args)]
#[clap(name = "rpc", about = "Make a raw RPC call")]
pub struct RpcCommand {
    /// The name of the method to call.
    method: String,
    /// The arguments of the method to call.
    #[clap(num_args = 0..)]
    params: Vec<String>,
    /// Websockets url of a substrate node.
    #[clap(
        name = "url",
        long,
        value_parser,
        default_value = "ws://localhost:9944"
    )]
    url: url::Url,
}

impl RpcCommand {
    pub async fn run(&self) -> Result<(), ErrorVariant> {
        let request = RpcRequest::new(&self.url).await?;
        let params = RawParams::new(&self.params)?;

        let result = request
            .raw_call(&self.method, params)
            .await
            .map_err(|e| anyhow::anyhow!("Method call failed: {}", e))?;

        let json: serde_json::Value = serde_json::from_str(result.get())?;
        println!("{}", serde_json::to_string_pretty(&json)?);
        Ok(())
    }
}

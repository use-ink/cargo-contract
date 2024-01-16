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
    Rpc,
    RpcRequest,
};
use scale_value::{
    stringify::{
        from_str_custom,
        ParseError,
    },
    Value,
};
use subxt::backend::rpc::RpcParams;

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
    /// Secret key URI for the account deploying the contract.
    ///
    /// e.g.
    /// - for a dev account "//Alice"
    /// - with a password "//Alice///SECRET_PASSWORD"
    #[clap(name = "suri", long, short)]
    suri: String,
}

impl RpcCommand {
    pub async fn run(&self) -> Result<(), ErrorVariant> {
        let rpc = Rpc::new(&self.url).await?;
        let request = RpcRequest::new(rpc);
        let str_parser = from_str_custom();
        let str_parser = str_parser.add_custom_parser(custom_parse_hex);

        let params = self
            .params
            .iter()
            .map(|e| str_parser.parse(e).0)
            .collect::<Result<Vec<_>, ParseError>>()
            .map_err(|e| anyhow::anyhow!("Function arguments parsing failed: {e}"))?;

        let params = match params.is_empty() {
            true => None,
            false => {
                params
                    .iter()
                    .try_fold(RpcParams::new(), |mut v, e| {
                        v.push(e)?;
                        Ok(v)
                    })
                    .map_err(|e: subxt::Error| {
                        anyhow::anyhow!("Method arguments parsing failed: {e}")
                    })?
                    .build()
            }
        };

        // println!("params: {:?}", params);
        let res = request
            .raw_call(&self.method, params)
            .await
            .map_err(|e| anyhow::anyhow!("Method call failed: {}", e))?;
        let json: serde_json::Value = serde_json::from_str(res.get())?;
        println!("{}", serde_json::to_string_pretty(&json)?);
        Ok(())
    }
}

pub fn custom_parse_hex(s: &mut &str) -> Option<Result<Value<()>, ParseError>> {
    if !s.starts_with("0x") {
        return None
    }
    Some(Ok(Value::string(s.to_string())))
}

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

use subxt::{
    backend::{
        legacy::LegacyRpcMethods,
        rpc::{
            RawValue,
            RpcClient,
            RpcParams,
        },
    },
    ext::scale_value::{
        stringify::{
            from_str_custom,
            ParseError,
        },
        Value,
    },
    Config,
    OnlineClient,
    PolkadotConfig,
};

use crate::url_to_string;
use anyhow::{
    anyhow,
    bail,
    Result,
};

pub struct RpcRequest {
    rpc: Rpc<PolkadotConfig>,
}

impl RpcRequest {
    pub fn new(rpc: Rpc<PolkadotConfig>) -> Self {
        Self { rpc }
    }

    pub async fn raw_call<'a>(
        &'a self,
        method: &'a str,
        params: &[String],
    ) -> Result<Box<RawValue>> {
        let methods = self.get_supported_methods().await?;
        if !methods.iter().any(|e| e == method) {
            bail!(
                "Method not found, supported methods: {}",
                methods.join(", ")
            );
        }
        self.rpc
            .rpc_client
            .request_raw(method, Self::params_to_rawvalue(params)?)
            .await
            .map_err(|e| anyhow!("Raw RPC call failed: {e}"))
    }

    pub async fn get_supported_methods(&self) -> Result<Vec<String>> {
        let result = self
            .rpc
            .rpc_client
            .request_raw("rpc_methods", None)
            .await
            .map_err(|e| anyhow!("Rpc method call failed: {e}"))?;

        let result_value: serde_json::Value = serde_json::from_str(result.get())?;

        let methods = result_value
            .get("methods")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("Methods parsing failed!"))?;
        let patterns = ["watch", "unstable", "subscribe"];
        let filtered_methods: Vec<String> = methods
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .filter(|s| {
                patterns
                    .iter()
                    .all(|&pattern| !s.to_lowercase().contains(pattern))
            })
            .collect();

        Ok(filtered_methods)
    }

    fn params_to_rawvalue(params: &[String]) -> Result<Option<Box<RawValue>>> {
        let mut str_parser = from_str_custom();
        str_parser = str_parser.add_custom_parser(Self::custom_parse_hex);

        let params = params
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

        println!("params: {:?}", params);
        Ok(params)
    }

    /// Parse hex to string
    fn custom_parse_hex(s: &mut &str) -> Option<Result<Value<()>, ParseError>> {
        if !s.starts_with("0x") {
            return None
        }

        let end_idx = s
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(s.len());
        let hex = &s[0..end_idx];
        *s = &s[end_idx..];
        Some(Ok(Value::string(hex.to_string())))
    }
}

/// Methods for querying over RPC.
pub struct Rpc<C: Config> {
    rpc_client: RpcClient,
    _rpc_methods: LegacyRpcMethods<C>,
    _client: OnlineClient<C>,
}

impl<C: Config> Rpc<C> {
    /// Create a new instance of the Rpc.
    pub async fn new(url: &url::Url) -> Result<Self> {
        let rpc_client = RpcClient::from_url(url_to_string(url)).await?;
        let _client = OnlineClient::from_rpc_client(rpc_client.clone()).await?;
        let _rpc_methods = LegacyRpcMethods::new(rpc_client.clone());
        Ok(Self {
            rpc_client,
            _rpc_methods,
            _client,
        })
    }
}

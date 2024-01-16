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
        },
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
        params: Option<Box<RawValue>>,
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
            .request_raw(method, params)
            .await
            .map_err(|e| anyhow!("Raw RPC call failed: {e}"))
    }

    pub async fn get_supported_methods(&self) -> Result<Vec<String>> {
        let raw_value = self
            .rpc
            .rpc_client
            .request_raw("rpc_methods", None)
            .await
            .map_err(|e| anyhow!("Rpc method call failed: {e}"))?;

        let value: serde_json::Value = serde_json::from_str(raw_value.get())?;

        let methods = value
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

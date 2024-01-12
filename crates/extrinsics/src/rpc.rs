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

use subxt::backend::rpc::{
    RawRpcFuture,
    RawValue,
    RpcClient,
};

use crate::url_to_string;

use anyhow::Result;

pub struct RpcRequest {
    rpc: Rpc,
}

impl RpcRequest {
    pub fn new(rpc: Rpc) -> Self {
        Self { rpc }
    }

    pub fn exec<'a>(
        &'a self,
        method: &'a str,
        params: Option<Box<RawValue>>,
    ) -> RawRpcFuture<'a, Box<RawValue>> {
        self.rpc.rpc_client.request_raw(method, params)
    }
}

/// Methods for querying over RPC.
pub struct Rpc {
    rpc_client: RpcClient,
}

impl Rpc {
    /// Create a new instance of the Rpc.
    pub async fn new(url: &url::Url) -> Result<Self> {
        let rpc_client = RpcClient::from_url(url_to_string(url)).await?;
        Ok(Self { rpc_client })
    }
}

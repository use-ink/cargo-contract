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

use super::{
    config::SignerConfig,
    parse_addr,
    CLIChainOpts,
};
use crate::{
    call_with_config,
    ErrorVariant,
};
use anyhow::Result;
use contract_extrinsics::{
    resolve_h160,
    url_to_string,
};
use ink_env::Environment;
use serde::Serialize;
use std::{
    fmt::{
        Debug,
        Display,
    },
    str::FromStr,
};
use subxt::{
    backend::{
        legacy::LegacyRpcMethods,
        rpc::RpcClient,
    },
    config::HashFor,
    ext::{
        codec::Decode,
        scale_decode::IntoVisitor,
    },
    Config,
    OnlineClient,
};

#[derive(Debug, clap::Args)]
#[clap(
    name = "account",
    about = "Map and unmap accounts, display info about addresses"
)]
pub struct AccountCommand {
    /// An H160 address to display the AccountId for.
    #[clap(name = "addr", long, env = "ADDR")]
    addr: Option<String>,
    /// Export the output in JSON format.
    #[clap(long)]
    output_json: bool,
    /// Arguments required for communicating with a Substrate node.
    #[clap(flatten)]
    chain_cli_opts: CLIChainOpts,
}

impl AccountCommand {
    pub async fn handle(&self) -> Result<(), ErrorVariant> {
        call_with_config!(self, run, self.chain_cli_opts.chain().config())
    }

    async fn run<C: Config + Environment + SignerConfig<C>>(
        &self,
    ) -> Result<(), ErrorVariant>
    where
        <C as Config>::AccountId:
            Serialize + Display + IntoVisitor + Decode + AsRef<[u8]> + FromStr,
        HashFor<C>: IntoVisitor + Display,
        <C as Environment>::Balance: Serialize + Debug + IntoVisitor,
        <<C as Config>::AccountId as FromStr>::Err:
            Into<Box<dyn std::error::Error>> + Display,
    {
        let rpc_cli =
            RpcClient::from_url(url_to_string(&self.chain_cli_opts.chain().url()))
                .await
                .map_err(|e| subxt::Error::Rpc(e.into()))?;
        let client = OnlineClient::<C>::from_rpc_client(rpc_cli.clone()).await?;
        let rpc = LegacyRpcMethods::<C>::new(rpc_cli.clone());

        // Contract arg shall be always present in this case, it is enforced by
        // clap configuration
        let addr = self
            .addr
            .as_ref()
            .map(|c| parse_addr(c))
            .transpose()?
            .expect("Contract argument shall be present");

        let account_id = resolve_h160::<C, C>(&addr, &rpc, &client).await?;

        // todo display account balance as well
        //let deposit_account_data =
        //get_account_balance::<C, E>(account, rpc, client).await?;

        if self.output_json {
            let output = serde_json::json!({
                "account_id": format!("0x{}", hex::encode(account_id))
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("{}", account_id);
        }
        Ok(())
    }
}

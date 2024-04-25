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

use contract_build::name_value_println;
use contract_extrinsics::{
    ErrorVariant,
    RawParams,
    RpcRequest,
};
use subxt::ext::scale_value;

use super::{
    CLIChainOpts,
    MAX_KEY_COL_WIDTH,
};

#[derive(Debug, clap::Args)]
#[clap(name = "rpc", about = "Make a raw RPC call")]
pub struct RpcCommand {
    /// The name of the method to call.
    method: String,
    /// The arguments of the method to call.
    #[clap(num_args = 0..)]
    params: Vec<String>,
    /// Export the call output in JSON format.
    #[clap(long)]
    output_json: bool,
    /// Arguments required for communicating with a Substrate node.
    #[clap(flatten)]
    chain_cli_opts: CLIChainOpts,
}

impl RpcCommand {
    pub async fn run(&self) -> Result<(), ErrorVariant> {
        let request = RpcRequest::new(&self.chain_cli_opts.chain().url()).await?;
        let params = RawParams::new(&self.params)?;

        let result = request.raw_call(&self.method, params).await;

        match (result, self.output_json) {
            (Err(err), false) => Err(anyhow::anyhow!("Method call failed: {}", err))?,
            (Err(err), true) => {
                Err(anyhow::anyhow!(serde_json::to_string_pretty(
                    &ErrorVariant::from(err)
                )?))?
            }
            (Ok(res), false) => {
                let output: scale_value::Value = serde_json::from_str(res.get())?;
                name_value_println!("Result", output, MAX_KEY_COL_WIDTH);
                Ok(())
            }
            (Ok(res), true) => {
                let json: serde_json::Value = serde_json::from_str(res.get())?;
                println!("{}", serde_json::to_string_pretty(&json)?);
                Ok(())
            }
        }
    }
}

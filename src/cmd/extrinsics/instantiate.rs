// Copyright 2018-2020 Parity Technologies (UK) Ltd.
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

use crate::ExtrinsicOpts;
use anyhow::Result;
use structopt::StructOpt;
use subxt::{
    balances::Balances, contracts::*, system::System, ClientBuilder, ContractsTemplateRuntime,
};

#[derive(Debug, StructOpt)]
#[structopt(name = "instantiate", about = "Instantiate a contract")]
pub struct InstantiateCommand {
    /// The name of the contract constructor to call
    name: String,
    /// The constructor arguments, encoded as strings
    args: Vec<String>,
    #[structopt(flatten)]
    extrinsic_opts: ExtrinsicOpts,
    /// Transfers an initial balance to the instantiated contract
    #[structopt(name = "endowment", long, default_value = "0")]
    endowment: <ContractsTemplateRuntime as Balances>::Balance,
    /// Maximum amount of gas to be used for this command
    #[structopt(name = "gas", long, default_value = "5000000000")]
    gas_limit: u64,
    /// The hash of the smart contract code already uploaded to the chain
    #[structopt(long, parse(try_from_str = parse_code_hash))]
    code_hash: <ContractsTemplateRuntime as System>::Hash,
}

impl InstantiateCommand {
    /// Instantiate a contract stored at the supplied code hash.
    /// Returns the account id of the instantiated contract if successful.
    ///
    /// Creates an extrinsic with the `Contracts::instantiate` Call, submits via RPC, then waits for
    /// the `ContractsEvent::Instantiated` event.
    pub fn run(&self) -> Result<<ContractsTemplateRuntime as System>::Address> {
        let metadata = super::load_metadata()?;
        let transcoder = super::Transcoder::new(metadata);
        let data = transcoder.encode(&self.name, &self.args)?;

        async_std::task::block_on(async move {
            let cli = ClientBuilder::<ContractsTemplateRuntime>::new()
                .set_url(self.extrinsic_opts.url.to_string())
                .build()
                .await?;
            let signer = self.extrinsic_opts.signer()?;

            let events = cli
                .instantiate_and_watch(
                    &signer,
                    self.endowment,
                    self.gas_limit,
                    &self.code_hash,
                    &data,
                )
                .await?;

            for event in &events.events {
                println!("{}:{}", event.module, event.variant);
            }

            let instantiated = events
                .instantiated()?
                .ok_or(anyhow::anyhow!("Failed to find Instantiated event"))?;

            Ok(instantiated.contract)
        })
    }
}

#[cfg(feature = "extrinsics")]
fn parse_code_hash(input: &str) -> Result<<ContractsTemplateRuntime as System>::Hash> {
    let bytes = hex::decode(input)?;
    if bytes.len() != 32 {
        anyhow::bail!("Code hash should be 32 bytes in length")
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cmd::execute_deploy, util::tests::with_tmp_dir, ExtrinsicOpts};
    use assert_matches::assert_matches;
    use std::{fs, io::Write};

    const CONTRACT: &str = r#"
(module
    (func (export "call"))
    (func (export "deploy"))
)
"#;

    #[test]
    #[ignore] // depends on a local substrate node running
    fn instantiate_contract() {
        with_tmp_dir(|path| {
            let wasm = wabt::wat2wasm(CONTRACT).expect("invalid wabt");

            let wasm_path = path.join("test.wasm");
            let mut file = fs::File::create(&wasm_path).unwrap();
            let _ = file.write_all(&wasm);

            let url = url::Url::parse("ws://localhost:9944").unwrap();
            let extrinsic_opts = ExtrinsicOpts {
                url,
                suri: "//Alice".into(),
                password: None,
            };
            let code_hash =
                execute_deploy(&extrinsic_opts, Some(&wasm_path)).expect("Deploy should succeed");

            let cmd = InstantiateCommand {
                extrinsic_opts,
                endowment: 100000000000000,
                gas_limit: 500_000_000,
                code_hash,
                name: String::new(), // todo: does this invoke the default constructor?
                args: Vec::new(),
            };
            let result = cmd.run();

            assert_matches!(result, Ok(_));
            Ok(())
        })
    }
}

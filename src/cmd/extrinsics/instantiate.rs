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

use super::{
    display_events,
    runtime_api::{api, ContractsRuntime},
};
use crate::{util::decode_hex, ExtrinsicOpts};
use anyhow::Result;
use structopt::StructOpt;
use subxt::{ClientBuilder, Runtime};

#[derive(Debug, StructOpt)]
pub struct InstantiateArgs {
    /// The name of the contract constructor to call
    #[structopt(name = "constructor", long, default_value = "new")]
    pub(super) constructor: String,
    /// The constructor parameters, encoded as strings
    #[structopt(name = "params", long, default_value = "new")]
    pub(super) params: Vec<String>,
    #[structopt(flatten)]
    pub(super) extrinsic_opts: ExtrinsicOpts,
    /// Transfers an initial balance to the instantiated contract
    #[structopt(name = "endowment", long, default_value = "0")]
    pub(super) endowment: super::Balance,
    /// Maximum amount of gas to be used for this command
    #[structopt(name = "gas", long, default_value = "50000000000")]
    pub(super) gas_limit: u64,
    // todo: [AJ] add salt
}

#[derive(Debug, StructOpt)]
#[structopt(name = "instantiate", about = "Instantiate a contract")]
pub struct InstantiateCommand {
    #[structopt(flatten)]
    instantiate: InstantiateArgs,
    /// The hash of the smart contract code already uploaded to the chain
    #[structopt(long, parse(try_from_str = parse_code_hash))]
    code_hash: <ContractsRuntime as Runtime>::Hash,
}

impl InstantiateCommand {
    /// Instantiate a contract stored at the supplied code hash.
    /// Returns the account id of the instantiated contract if successful.
    ///
    /// Creates an extrinsic with the `Contracts::instantiate` Call, submits via RPC, then waits for
    /// the `ContractsEvent::Instantiated` event.
    pub fn run(&self) -> Result<<ContractsRuntime as Runtime>::AccountId> {
        let metadata = super::load_metadata()?;
        let transcoder = super::ContractMessageTranscoder::new(&metadata);
        let data = transcoder.encode(&self.instantiate.constructor, &self.instantiate.params)?;

        async_std::task::block_on(async move {
            let cli = ClientBuilder::new()
                .set_url(self.instantiate.extrinsic_opts.url.to_string())
                .build()
                .await?;
            let api = api::RuntimeApi::new(cli);
            let signer = super::pair_signer(self.instantiate.extrinsic_opts.signer()?);

            let extrinsic = api.tx.contracts.instantiate(
                self.instantiate.endowment,
                self.instantiate.gas_limit,
                self.code_hash,
                data,
                vec![], // todo: [AJ] salt
            );
            let result = extrinsic.sign_and_submit_then_watch(&signer).await?;

            display_events(
                &result,
                &transcoder,
                self.instantiate.extrinsic_opts.verbosity()?,
            );

            let instantiated = result
                .find_event::<api::contracts::events::Instantiated>()?
                .ok_or(anyhow::anyhow!("Failed to find Instantiated event"))?;

            Ok(instantiated.0)
        })
    }
}

fn parse_code_hash(input: &str) -> Result<<ContractsRuntime as Runtime>::Hash> {
    let bytes = decode_hex(input)?;
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
    use crate::{
        cmd::InstantiateWithCode, util::tests::with_tmp_dir, ExtrinsicOpts, VerbosityFlags,
    };
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
                verbosity: VerbosityFlags::quiet(),
            };
            let deploy = InstantiateWithCode {
                extrinsic_opts: extrinsic_opts.clone(),
                wasm_path: Some(wasm_path),
            };
            let code_hash = deploy.exec().expect("Deploy should succeed");

            let cmd = InstantiateCommand {
                extrinsic_opts,
                endowment: 100000000000000,
                gas_limit: 500_000_000,
                code_hash,
                name: String::new(), // todo: does this invoke the default constructor?
                instantiate: Vec::new(),
            };
            let result = cmd.run();

            assert_matches!(result, Ok(_));
            Ok(())
        })
    }

    #[test]
    fn parse_code_hash_works() {
        // with 0x prefix
        assert!(parse_code_hash(
            "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
        )
        .is_ok());
        // without 0x prefix
        assert!(
            parse_code_hash("d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d")
                .is_ok()
        )
    }
}

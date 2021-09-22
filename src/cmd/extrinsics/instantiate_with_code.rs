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

use anyhow::{Context, Result};
use sp_core::H256;
use std::{fs, io::Read, path::PathBuf};
use structopt::StructOpt;
use subxt::{ClientBuilder, Runtime};

use super::{
    display_events,
    instantiate::InstantiateArgs,
    load_metadata,
    runtime_api::{api, ContractsRuntime},
    ContractMessageTranscoder,
};
use crate::crate_metadata;

#[derive(Debug, StructOpt)]
#[structopt(name = "deploy", about = "Upload contract wasm")]
pub struct InstantiateWithCode {
    #[structopt(flatten)]
    instantiate: InstantiateArgs,
    /// Path to wasm contract code, defaults to `./target/ink/<name>.wasm`
    #[structopt(parse(from_os_str))]
    pub(super) wasm_path: Option<PathBuf>,
}

impl InstantiateWithCode {
    /// Load the wasm blob from the specified path.
    ///
    /// Defaults to the target contract wasm in the current project, inferred via the crate metadata.
    fn load_contract_code(&self) -> Result<Vec<u8>> {
        let contract_wasm_path = match self.wasm_path {
            Some(ref path) => path.clone(),
            None => {
                let metadata = crate_metadata::CrateMetadata::collect(&Default::default())?;
                metadata.dest_wasm
            }
        };
        log::info!("Contract code path: {}", contract_wasm_path.display());
        let mut data = Vec::new();
        let mut file = fs::File::open(&contract_wasm_path)
            .context(format!("Failed to open {}", contract_wasm_path.display()))?;
        file.read_to_end(&mut data)?;

        Ok(data)
    }

    /// Put contract code to a smart contract enabled substrate chain.
    /// Returns the code hash of the deployed contract if successful.
    ///
    /// Optionally supply the contract wasm path, defaults to destination contract file inferred from
    /// Cargo.toml of the current contract project.
    ///
    /// Creates an extrinsic with the `Contracts::put_code` Call, submits via RPC, then waits for
    /// the `ContractsEvent::CodeStored` event.
    pub fn exec(&self) -> Result<(H256, <ContractsRuntime as Runtime>::AccountId)> {
        let code = self.load_contract_code()?;
        let metadata = load_metadata()?;
        let transcoder = ContractMessageTranscoder::new(&metadata);
        let data = transcoder.encode(&self.instantiate.constructor, &self.instantiate.params)?;

        async_std::task::block_on(async move {
            let cli = ClientBuilder::new()
                .set_url(&self.instantiate.extrinsic_opts.url.to_string())
                .build()
                .await?;
            let api = api::RuntimeApi::new(cli);
            let signer = super::pair_signer(self.instantiate.extrinsic_opts.signer()?);

            let extrinsic = api.tx.contracts.instantiate_with_code(
                self.instantiate.endowment,
                self.instantiate.gas_limit,
                code,
                data,
                vec![], // todo! [AJ] add salt
            );
            let result = extrinsic.sign_and_submit_then_watch(&signer).await?;

            display_events(
                &result,
                &transcoder,
                self.instantiate.extrinsic_opts.verbosity()?,
            );

            let code_stored = result
                .find_event::<api::contracts::events::CodeStored>()?
                .ok_or(anyhow::anyhow!("Failed to find CodeStored event"))?;

            let instantiated = result
                .find_event::<api::contracts::events::Instantiated>()?
                .ok_or(anyhow::anyhow!("Failed to find Instantiated event"))?;

            Ok((code_stored.0, instantiated.0))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Write};

    use crate::{util::tests::with_tmp_dir, ExtrinsicOpts, VerbosityFlags};
    use assert_matches::assert_matches;

    const CONTRACT: &str = r#"
(module
    (func (export "call"))
    (func (export "deploy"))
)
"#;

    #[test]
    #[ignore] // depends on a local substrate node running
    fn deploy_contract() {
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
            let cmd = InstantiateWithCode {
                extrinsic_opts,
                wasm_path: Some(wasm_path),
            };
            let result = cmd.exec();

            assert_matches!(result, Ok(_));
            Ok(())
        })
    }
}

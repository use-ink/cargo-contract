// Copyright 2018-2021 Parity Technologies (UK) Ltd.
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

use anyhow::Result;
use subxt::{balances::Balances, contracts::*, system::System, ClientBuilder, DefaultNodeRuntime};

use crate::{ExtrinsicOpts, HexData};

/// Instantiate a contract stored at the supplied code hash.
/// Returns the account id of the instantiated contract if successful.
///
/// Creates an extrinsic with the `Contracts::instantiate` Call, submits via RPC, then waits for
/// the `ContractsEvent::Instantiated` event.
pub(crate) fn execute_instantiate(
    extrinsic_opts: &ExtrinsicOpts,
    endowment: <DefaultNodeRuntime as Balances>::Balance,
    gas_limit: u64,
    code_hash: <DefaultNodeRuntime as System>::Hash,
    data: HexData,
) -> Result<<DefaultNodeRuntime as System>::AccountId> {
    async_std::task::block_on(async move {
        let cli = ClientBuilder::<DefaultNodeRuntime>::new()
            .set_url(&extrinsic_opts.url.to_string())
            .build()
            .await?;
        let signer = extrinsic_opts.signer()?;

        let events = cli
            .instantiate_and_watch(&signer, endowment, gas_limit, &code_hash, &data.0)
            .await?;
        let instantiated = events
            .instantiated()?
            .ok_or(anyhow::anyhow!("Failed to find Instantiated event"))?;

        Ok(instantiated.contract)
    })
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use crate::{cmd::deploy::execute_deploy, util::tests::with_tmp_dir, ExtrinsicOpts, HexData};
    use assert_matches::assert_matches;

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

            let gas_limit = 500_000_000;
            let result = super::execute_instantiate(
                &extrinsic_opts,
                100000000000000,
                gas_limit,
                code_hash,
                HexData::default(),
            );

            assert_matches!(result, Ok(_));
            Ok(())
        })
    }
}

// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of ink!.
//
// ink! is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// ink! is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with ink!.  If not, see <http://www.gnu.org/licenses/>.

use anyhow::Result;
use futures::future::Future;
use subxt::{balances::Balances, contracts, system::System, DefaultNodeRuntime};

use crate::{ExtrinsicOpts, HexData};

/// Attempt to extract the contract account from the extrinsic result.
///
/// Returns an Error if the `Contracts::Instantiated` is not found or cannot be decoded.
fn extract_contract_account<T: System>(
    extrinsic_result: subxt::ExtrinsicSuccess<T>,
) -> Result<T::AccountId> {
    match extrinsic_result.find_event::<(T::AccountId, T::AccountId)>("Contracts", "Instantiated") {
        Some(Ok((_src_acct, dest_acct))) => Ok(dest_acct),
        Some(Err(err)) => Err(anyhow::anyhow!(
            "Failed to decode contract source and destination accounts: {}",
            err
        )),
        None => Err(anyhow::anyhow!(
            "Failed to find Contracts::Instantiated Event"
        )),
    }
}

/// Instantiate a contract stored at the supplied code hash.
/// Returns the account id of the instantiated contract if successful.
///
/// Creates an extrinsic with the `Contracts::instantiate` Call, submits via RPC, then waits for
/// the `ContractsEvent::Instantiated` event.
pub(crate) fn execute_instantiate(
    extrinsic_opts: &ExtrinsicOpts,
    endowment: <DefaultNodeRuntime as Balances>::Balance,
    code_hash: <DefaultNodeRuntime as System>::Hash,
    data: HexData,
) -> Result<<DefaultNodeRuntime as System>::AccountId> {
    let signer = extrinsic_opts.signer()?;
    let gas_limit = extrinsic_opts.gas_limit.clone();

    let fut = subxt::ClientBuilder::<DefaultNodeRuntime>::new()
        .set_url(extrinsic_opts.url.clone())
        .build()
        .and_then(|cli| cli.xt(signer, None))
        .and_then(move |xt| {
            xt.watch()
                .submit(contracts::instantiate::<DefaultNodeRuntime>(
                    endowment, gas_limit, code_hash, data.0,
                ))
        });

    let mut rt = tokio::runtime::Runtime::new()?;
    if let Ok(extrinsic_success) = rt.block_on(fut) {
        log::debug!("Instantiate success: {:?}", extrinsic_success);

        extract_contract_account(extrinsic_success)
    } else {
        Err(anyhow::anyhow!("Deploy error"))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use crate::{
        cmd::{deploy::execute_deploy, tests::with_tmp_dir},
        ExtrinsicOpts, HexData,
    };
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
                gas_limit: 500_000,
            };
            let code_hash =
                execute_deploy(&extrinsic_opts, Some(&wasm_path)).expect("Deploy should succeed");

            let result = super::execute_instantiate(
                &extrinsic_opts,
                100000000000000,
                code_hash,
                HexData::default(),
            );

            assert_matches!(result, Ok(_));
        });
    }
}

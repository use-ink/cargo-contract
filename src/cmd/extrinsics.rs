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
use subxt::{ClientBuilder, DefaultNodeRuntime, ExtrinsicSuccess};

use crate::ExtrinsicOpts;

/// Submits an extrinsic to a substrate node, waits for it to succeed and returns an event expected
/// to have been triggered by the extrinsic.
pub(crate) fn submit_extrinsic<C, E>(
	extrinsic_opts: &ExtrinsicOpts,
	call: subxt::Call<C>,
	event_mod: &str,
	event_name: &str,
) -> Result<E>
	where
		C: codec::Encode,
		E: codec::Decode,
{
	let result: Result<ExtrinsicSuccess<_>> = async_std::task::block_on(async move {
		let cli = ClientBuilder::<DefaultNodeRuntime>::new()
			.set_url(&extrinsic_opts.url.to_string())
			.build()
			.await?;
		let signer = extrinsic_opts.signer()?;
		let xt = cli.xt(signer, None).await?;
		let success = xt.watch().submit(call).await?;
		Ok(success)
	});

	match result?.find_event::<E>(event_mod, event_name) {
		Some(Ok(hash)) => Ok(hash),
		Some(Err(err)) => Err(anyhow::anyhow!(
            "Failed to decode event '{} {}': {}",
            event_mod,
            event_name,
            err
        )),
		None => Err(anyhow::anyhow!(
            "Failed to find '{} {}' Event",
            event_mod,
            event_name
        )),
	}
}

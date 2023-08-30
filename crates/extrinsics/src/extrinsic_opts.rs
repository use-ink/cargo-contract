// Copyright 2018-2023 Parity Technologies (UK) Ltd.
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

use core::marker::PhantomData;

use subxt_signer::{
    sr25519::Keypair,
    SecretUri,
};
use url::Url;

use anyhow::{
    Ok,
    Result,
};

use crate::{
    Balance,
    BalanceVariant,
    ContractArtifacts,
    TokenMetadata,
};
use std::{
    option::Option,
    path::PathBuf,
};

/// Arguments required for creating and sending an extrinsic to a substrate node.
#[derive(Clone, Debug)]
pub struct ExtrinsicOpts {
    file: Option<PathBuf>,
    manifest_path: Option<PathBuf>,
    url: url::Url,
    suri: String,
    storage_deposit_limit: Option<BalanceVariant>,
}

/// Type state for the extrinsics' commands to tell that some mandatory state has not yet
/// been set yet or to fail upon setting the same state multiple times.
pub struct Missing<S>(PhantomData<fn() -> S>);

pub mod state {
    //! Type states that tell what state of the commands has not
    //! yet been set properly for a valid construction.

    /// Type state for the Secret key URI.
    pub struct Suri;
    /// Type state for extrinsic options.
    pub struct ExtrinsicOptions;
    /// Type state for the name of the contract message to call.
    pub struct Message;
}

/// A builder for extrinsic options.
pub struct ExtrinsicOptsBuilder<Suri> {
    opts: ExtrinsicOpts,
    marker: PhantomData<fn() -> Suri>,
}

impl ExtrinsicOptsBuilder<Missing<state::Suri>> {
    /// Returns a clean builder for `ExtrinsicOpts`.
    pub fn new() -> ExtrinsicOptsBuilder<Missing<state::Suri>> {
        ExtrinsicOptsBuilder {
            opts: ExtrinsicOpts::default(),
            marker: PhantomData,
        }
    }

    /// Sets the secret key URI for the account deploying the contract.
    pub fn suri<T: Into<String>>(self, suri: T) -> ExtrinsicOptsBuilder<state::Suri> {
        ExtrinsicOptsBuilder {
            opts: ExtrinsicOpts {
                suri: suri.into(),
                ..self.opts
            },
            marker: PhantomData,
        }
    }
}

impl Default for ExtrinsicOptsBuilder<Missing<state::Suri>> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> ExtrinsicOptsBuilder<S> {
    /// Sets the path to the contract build artifact file.
    pub fn file<T: Into<PathBuf>>(self, file: Option<T>) -> Self {
        let mut this = self;
        this.opts.file = file.map(|f| f.into());
        this
    }

    /// Sets the path to the Cargo.toml of the contract.
    pub fn manifest_path<T: Into<PathBuf>>(self, manifest_path: Option<T>) -> Self {
        let mut this = self;
        this.opts.manifest_path = manifest_path.map(|f| f.into());
        this
    }

    /// Sets the websockets url of a substrate node.
    pub fn url<T: Into<Url>>(self, url: T) -> Self {
        let mut this = self;
        this.opts.url = url.into();
        this
    }

    /// Sets the maximum amount of balance that can be charged from the caller to pay for
    /// storage.
    pub fn storage_deposit_limit(
        self,
        storage_deposit_limit: Option<BalanceVariant>,
    ) -> Self {
        let mut this = self;
        this.opts.storage_deposit_limit = storage_deposit_limit;
        this
    }
}

impl ExtrinsicOptsBuilder<state::Suri> {
    /// Finishes construction of the extrinsic options.
    pub fn done(self) -> ExtrinsicOpts {
        self.opts
    }
}

#[allow(clippy::new_ret_no_self)]
impl ExtrinsicOpts {
    /// Returns a clean builder for [`ExtrinsicOpts`].
    pub fn new() -> ExtrinsicOptsBuilder<Missing<state::Suri>> {
        ExtrinsicOptsBuilder {
            opts: Self {
                file: None,
                manifest_path: None,
                url: url::Url::parse("ws://localhost:9944").unwrap(),
                suri: String::new(),
                storage_deposit_limit: None,
            },
            marker: PhantomData,
        }
    }

    /// Load contract artifacts.
    pub fn contract_artifacts(&self) -> Result<ContractArtifacts> {
        ContractArtifacts::from_manifest_or_file(
            self.manifest_path.as_ref(),
            self.file.as_ref(),
        )
    }

    /// Returns the signer for contract extrinsics.
    pub fn signer(&self) -> Result<Keypair> {
        let uri = <SecretUri as std::str::FromStr>::from_str(&self.suri)?;
        let keypair = Keypair::from_uri(&uri)?;
        Ok(keypair)
    }

    /// Convert URL to String without omitting the default port
    pub fn url_to_string(&self) -> String {
        let mut res = self.url.to_string();
        match (self.url.port(), self.url.port_or_known_default()) {
            (None, Some(port)) => {
                res.insert_str(res.len() - 1, &format!(":{port}"));
                res
            }
            _ => res,
        }
    }

    /// Return the file path of the contract artifact.
    pub fn file(&self) -> Option<&PathBuf> {
        self.file.as_ref()
    }

    /// Return the path to the `Cargo.toml` of the contract.
    pub fn manifest_path(&self) -> Option<&PathBuf> {
        self.manifest_path.as_ref()
    }

    /// Return the URL of the substrate node.
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Return the secret URI of the signer.
    pub fn suri(&self) -> &str {
        &self.suri
    }

    /// Return the storage deposit limit.
    pub fn storage_deposit_limit(&self) -> Option<&BalanceVariant> {
        self.storage_deposit_limit.as_ref()
    }

    /// Get the storage deposit limit converted to compact for passing to extrinsics.
    pub fn compact_storage_deposit_limit(
        &self,
        token_metadata: &TokenMetadata,
    ) -> Result<Option<scale::Compact<Balance>>> {
        Ok(self
            .storage_deposit_limit
            .as_ref()
            .map(|bv| bv.denominate_balance(token_metadata))
            .transpose()?
            .map(Into::into))
    }
}

impl Default for ExtrinsicOpts {
    fn default() -> Self {
        ExtrinsicOpts::new().suri("Alice".to_string()).done()
    }
}

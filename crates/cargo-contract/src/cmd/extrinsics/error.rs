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

use std::fmt::Display;
use subxt::ext::sp_runtime::DispatchError;

#[derive(serde::Serialize)]
pub enum ErrorVariant {
    #[serde(rename = "module_error")]
    Module(ModuleError),
    #[serde(rename = "generic_error")]
    Generic(GenericError),
}

impl From<subxt::Error> for ErrorVariant {
    fn from(error: subxt::Error) -> Self {
        match error {
            subxt::Error::Runtime(subxt::error::DispatchError::Module(module_err)) => {
                ErrorVariant::Module(ModuleError {
                    pallet: module_err.pallet.clone(),
                    error: module_err.error.clone(),
                    docs: module_err.description,
                })
            }
            err => ErrorVariant::Generic(GenericError::from_message(err.to_string())),
        }
    }
}

impl From<anyhow::Error> for ErrorVariant {
    fn from(error: anyhow::Error) -> Self {
        Self::Generic(GenericError::from_message(format!("{error:?}")))
    }
}

impl From<&str> for ErrorVariant {
    fn from(err: &str) -> Self {
        Self::Generic(GenericError::from_message(err.to_owned()))
    }
}

#[derive(serde::Serialize)]
pub struct ModuleError {
    pub pallet: String,
    pub error: String,
    pub docs: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct GenericError {
    error: String,
}

impl GenericError {
    pub fn from_message(error: String) -> Self {
        GenericError { error }
    }
}

impl ErrorVariant {
    pub fn from_dispatch_error(
        error: &DispatchError,
        metadata: &subxt::Metadata,
    ) -> anyhow::Result<ErrorVariant> {
        match error {
            DispatchError::Module(err) => {
                let details = metadata.error(err.index, err.error[0])?;
                Ok(ErrorVariant::Module(ModuleError {
                    pallet: details.pallet().to_owned(),
                    error: details.error().to_owned(),
                    docs: details.docs().to_owned(),
                }))
            }
            err => {
                Ok(ErrorVariant::Generic(GenericError::from_message(format!(
                    "DispatchError: {err:?}"
                ))))
            }
        }
    }
}

impl Display for ErrorVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorVariant::Module(err) => {
                f.write_fmt(format_args!(
                    "ModuleError: {}::{}: {:?}",
                    err.pallet, err.error, err.docs
                ))
            }
            ErrorVariant::Generic(err) => write!(f, "{}", err.error),
        }
    }
}

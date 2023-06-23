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

use sp_runtime::DispatchError;
use std::fmt::{
    self,
    Debug,
    Display,
};

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
                module_err
                    .details()
                    .map(|details| {
                        ErrorVariant::Module(ModuleError {
                            pallet: details.pallet.name().to_string(),
                            error: details.variant.name.to_string(),
                            docs: details.variant.docs.clone(),
                        })
                    })
                    .unwrap_or_else(|err| {
                        ErrorVariant::Generic(GenericError::from_message(format!(
                            "Error extracting subxt error details: {}",
                            err
                        )))
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
                let pallet = metadata.pallet_by_index_err(err.index)?;
                let variant =
                    pallet.error_variant_by_index(err.error[0]).ok_or_else(|| {
                        anyhow::anyhow!("Error variant {} not found", err.error[0])
                    })?;
                Ok(ErrorVariant::Module(ModuleError {
                    pallet: pallet.name().to_string(),
                    error: variant.name.to_owned(),
                    docs: variant.docs.to_owned(),
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

impl Debug for ErrorVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        <Self as Display>::fmt(self, f)
    }
}

impl Display for ErrorVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

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
use std::{
    io::{self, Write},
    path::PathBuf,
    process::Command,
};

mod build;
mod cargo;
#[cfg(feature = "extrinsics")]
mod deploy;
#[cfg(feature = "extrinsics")]
mod extrinsics;
#[cfg(feature = "extrinsics")]
mod instantiate;
mod metadata;
mod new;

pub(crate) use self::{
    build::execute_build, cargo::exec_cargo, cargo::is_nightly, metadata::execute_generate_metadata, new::execute_new,
};
#[cfg(feature = "extrinsics")]
pub(crate) use self::{
    deploy::execute_deploy, extrinsics::submit_extrinsic, instantiate::execute_instantiate,
};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use tempfile::TempDir;

    pub fn with_tmp_dir<F: FnOnce(&PathBuf)>(f: F) {
        let tmp_dir = TempDir::new().expect("temporary directory creation failed");

        f(&tmp_dir.into_path());
    }
}

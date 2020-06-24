// Copyright 2018-2020 Parity Technologies (UK) Ltd.
// This file is part of cargo-contract.
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
use std::{fs, path::Path};
use toml::value;

pub(super) fn generate_package<P: AsRef<Path>>(
    dir: P,
    name: &str,
    ink_lang: value::Table,
    mut ink_abi: value::Table,
) -> Result<()> {
    let dir = dir.as_ref();
    log::debug!("Generating abi package for {} in {}", name, dir.display());

    let cargo_toml = include_str!("../../templates/tools/generate-metadata/_Cargo.toml");
    let main_rs = include_str!("../../templates/tools/generate-metadata/main.rs");

    let mut cargo_toml: value::Table = toml::from_str(cargo_toml)?;
    let deps = cargo_toml
        .get_mut("dependencies")
        .expect("[dependencies] section specified in the template")
        .as_table_mut()
        .expect("[dependencies] is a table specified in the template");

    // initialize contract dependency
    let contract = deps
        .get_mut("contract")
        .expect("contract dependency specified in the template")
        .as_table_mut()
        .expect("contract dependency is a table specified in the template");
    contract.insert("package".into(), name.into());

    // make ink_abi dependency use default features
    ink_abi.remove("default-features");
    ink_abi.remove("features");
    ink_abi.remove("optional");

    // add ink dependencies copied from contract manifest
    deps.insert("ink_lang".into(), ink_lang.into());
    deps.insert("ink_abi".into(), ink_abi.into());
    let cargo_toml = toml::to_string(&cargo_toml)?;

    fs::write(dir.join("Cargo.toml"), cargo_toml)?;
    fs::write(dir.join("main.rs"), main_rs)?;
    Ok(())
}

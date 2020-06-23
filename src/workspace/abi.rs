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

const CARGO_TOML: &str = r#"
[package]
name = "abi-gen"
version = "0.1.0"
authors = ["Parity Technologies <admin@parity.io>"]
edition = "2018"
publish = false

[[bin]]
name = "abi-gen"
path = "main.rs"

[dependencies]
contract = { path = "../.." }
serde = "1.0"
serde_json = "1.0"
"#;

const MAIN_RS: &str = r#"
extern crate contract;

extern "Rust" {
    fn __ink_generate_metadata() -> ink_abi::InkProject;
}

fn main() -> Result<(), std::io::Error> {
    let ink_project = unsafe { __ink_generate_metadata() };
    let contents = serde_json::to_string_pretty(&ink_project)?;
    std::fs::create_dir("target").ok();
    std::fs::write("target/metadata.json", contents)?;
    Ok(())
}
"#;

pub(super) fn generate_package<P: AsRef<Path>>(
    dir: P,
    name: &str,
    ink_lang: value::Table,
    mut ink_abi: value::Table,
) -> Result<()> {
    let dir = dir.as_ref();
    log::debug!("Generating abi package for {} in {}", name, dir.display());

    let mut cargo_toml: value::Table = toml::from_str(CARGO_TOML)?;
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
    fs::write(dir.join("main.rs"), MAIN_RS)?;
    Ok(())
}

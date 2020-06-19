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
use heck::CamelCase as _;
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
fn main() -> Result<(), std::io::Error> {
    let abi = <::contract::{{camel_name}} as ::ink_lang::GenerateAbi>::generate_abi();
    let contents = serde_json::to_string_pretty(&abi)?;
    std::fs::create_dir("target").ok();
    std::fs::write("target/metadata.json", contents)?;
    Ok(())
}
"#;

pub(super) fn generate_package<P: AsRef<Path>>(
    dir: P,
    name: &str,
    ink_lang: value::Table,
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

    // ink v2 compat
    if ink_lang.get("version") == Some(&value::Value::String("2".into())) {
        contract.insert("default-features".into(), false.into());
        contract.insert("features".into(), vec!["ink-generate-abi"].into());
    }

    // add ink_lang dependency
    deps.insert("ink_lang".into(), ink_lang.into());
    let cargo_toml = toml::to_string(&cargo_toml)?;

    // replace main.rs template placeholders
    let main_rs = MAIN_RS
        .replace("{{name}}", name)
        // todo: [AJ] can the contract struct name be accurately inferred and passed in?
        .replace("{{camel_name}}", &name.to_string().to_camel_case());

    fs::write(dir.join("Cargo.toml"), cargo_toml)?;
    fs::write(dir.join("main.rs"), main_rs)?;
    Ok(())
}

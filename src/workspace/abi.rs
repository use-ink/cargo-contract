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
use std::path::Path;

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
contract = { path = "../..", package = "{{name}}", default-features = false, features = ["ink-generate-abi"] }
ink_lang = { version = "2", git = "https://github.com/paritytech/ink", tag = "latest-v2", package = "ink_lang", default-features = false, features = ["ink-generate-abi"] }
serde = "1.0"
serde_json = "1.0"
"#;

const MAIN_RS: &str = r#"
fn main() -> Result<(), std::io::Error> {
    let abi = <contract::{{camel_name}} as ink_lang::GenerateAbi>::generate_abi();
    let contents = serde_json::to_string_pretty(&abi)?;
    std::fs::create_dir("target").ok();
    std::fs::write("target/metadata.json", contents)?;
    Ok(())
}
"#;

pub(super) fn generate_package<P: AsRef<Path>>(dir: P) -> Result<()> {
	todo!()
}

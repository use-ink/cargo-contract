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

/// Generates a cargo workspace package `metadata-gen` which will be invoked via `cargo run` to
/// generate contract metadata.
///
/// # Note
///
/// `ink!` dependencies are copied from the containing contract workspace to ensure the same
/// versions are utilized.
pub(super) fn generate_package<P: AsRef<Path>>(
    target_dir: P,
    contract_package_name: &str,
    ink_lang_dependency: value::Table,
    ink_metadata_dependency: value::Table,
) -> Result<()> {
    let dir = target_dir.as_ref();
    log::debug!(
        "Generating metadata package for {} in {}",
        contract_package_name,
        dir.display()
    );

    let main_rs = generate_main();
    let cargo_toml = generate_cargo_toml(contract_package_name, ink_lang_dependency, ink_metadata_dependency)?;

    fs::write(dir.join("Cargo.toml"), cargo_toml)?;
    fs::write(dir.join("main.rs"), main_rs)?;
    Ok(())
}

/// Generates the `Cargo.toml` file for the `metadata-gen` package
fn generate_cargo_toml(
    contract_package_name: &str,
    ink_lang_dependency: value::Table,
    mut ink_metadata_dependency: value::Table
) -> Result<String> {
    let template = include_str!("../../templates/tools/generate-metadata/_Cargo.toml");
    let mut cargo_toml: value::Table = toml::from_str(template)?;

    // get a mutable reference to the dependencies section
    let deps = cargo_toml
        .get_mut("dependencies")
        .expect("[dependencies] section specified in the template")
        .as_table_mut()
        .expect("[dependencies] is a table specified in the template");

    // initialize the contract dependency
    let contract = deps
        .get_mut("contract")
        .expect("contract dependency specified in the template")
        .as_table_mut()
        .expect("contract dependency is a table specified in the template");
    contract.insert("package".into(), contract_package_name.into());

    // make ink_metadata dependency use default features
    ink_metadata_dependency.remove("default-features");
    ink_metadata_dependency.remove("features");
    ink_metadata_dependency.remove("optional");

    // add ink dependencies copied from contract manifest
    deps.insert("ink_lang".into(), ink_lang_dependency.into());
    deps.insert("ink_metadata".into(), ink_metadata_dependency.into());

    let cargo_toml = toml::to_string(&cargo_toml)?;
    Ok(cargo_toml)
}

/// Generate a `main.rs` to invoke `__ink_generate_metadata`
fn generate_main() -> String {
    quote::quote! (
        extern crate contract;

        extern "Rust" {
            fn __ink_generate_metadata(
                extension: ::ink_metadata::InkProjectExtension
            ) -> ::ink_metadata::InkProject;
        }

        use ::ink_metadata::{
            Compiler,
            CompilerInfo,
            InkProjectContract,
            InkProjectExtension,
            InkProjectSource,
            InkProjectUser,
            Language,
            SourceCompiler,
            SourceLanguage,
            Version,
            Url,
        };

        fn main() -> Result<(), std::io::Error> {
            // todo: pass in the following as args to generate_main()
            let wasm_hash = [0u8; 32];
            let ink_version = Version::new(2, 1, 0);
            let rustc_version = Version::new(1, 46, 0);

            let extension = {
                let language = SourceLanguage::new(Language::Ink, ink_version.clone());
                let compiler = SourceCompiler::new(
                    CompilerInfo::new(Compiler::Ink, ink_version),
                    CompilerInfo::new(Compiler::RustC, rustc_version),
                );
                let source = InkProjectSource::new(
                    wasm_hash,
                    language,
                    compiler,
                );

                let contract =
                    InkProjectContract::build()
                        .name("testing")
                        .version(Version::new(0, 1, 0))
                        .authors(vec!["author@example.com"])
                        .documentation(Url::parse("http://example.com").unwrap())
                        .done();

                // todo: pass in user args
                let user: Option<InkProjectUser> = None;

                InkProjectExtension::new(source, contract, user)
            };

            let ink_project = unsafe { __ink_generate_metadata(extension) };

            let contents = serde_json::to_string_pretty(&ink_project)?;
            std::fs::create_dir("target").ok();
            std::fs::write("target/metadata.json", contents)?;
            Ok(())
        }
    ).to_string()
}

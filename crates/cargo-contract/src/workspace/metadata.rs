// Copyright 2018-2022 Parity Technologies (UK) Ltd.
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

use anyhow::Result;
use std::{
    fs,
    path::Path,
};
use toml::value;

/// Info for generating a metadata package.
pub struct MetadataPackage {
    contract_package_name: String,
    ink_event_metadata_externs: Vec<String>,
}

impl MetadataPackage {
    /// Construct a new [`MetadataPackage`].
    pub fn new(
        contract_package_name: String,
        ink_event_metadata_externs: Vec<String>,
    ) -> Self {
        Self {
            ink_event_metadata_externs,
            contract_package_name,
        }
    }

    /// Generates a cargo workspace package `metadata-gen` which will be invoked via `cargo run` to
    /// generate contract metadata.
    ///
    /// # Note
    ///
    /// `ink!` dependencies are copied from the containing contract workspace to ensure the same
    /// versions are utilized.
    pub fn generate<P: AsRef<Path>>(
        &self,
        target_dir: P,
        mut ink_crate_dependency: value::Table,
    ) -> Result<()> {
        let dir = target_dir.as_ref();
        tracing::debug!(
            "Generating metadata package for {} in {}",
            self.contract_package_name,
            dir.display()
        );

        let cargo_toml =
            include_str!("../../templates/tools/generate-metadata/_Cargo.toml");
        let main_rs = self.generate_main();

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
        contract.insert("package".into(), self.contract_package_name.clone().into());

        // make ink_metadata dependency use default features
        ink_crate_dependency.remove("default-features");
        ink_crate_dependency.remove("features");
        ink_crate_dependency.remove("optional");

        // add ink dependencies copied from contract manifest
        deps.insert("ink".into(), ink_crate_dependency.into());
        let cargo_toml = toml::to_string(&cargo_toml)?;

        fs::write(dir.join("Cargo.toml"), cargo_toml)?;
        fs::write(dir.join("main.rs"), main_rs.to_string())?;
        Ok(())
    }

    /// Generate the `main.rs` file to be executed to generate the metadata.
    fn generate_main(&self) -> proc_macro2::TokenStream {
        let ink_event_metadata_fns = self
            .ink_event_metadata_externs
            .iter()
            .map(|event_metadata_fn| quote::format_ident!("{}", event_metadata_fn))
            .collect::<Vec<_>>();

        quote::quote!(
            extern crate contract;

            extern "Rust" {
                // Note: The ink! metadata codegen generates an implementation for this function,
                // which is what we end up linking to here.
                fn __ink_generate_metadata(
                    events: ::ink::prelude::vec::Vec<::ink::metadata::EventSpec>
                ) -> ::ink::metadata::InkProject;

                // All `#[ink::event_definition]`s export a unique function to fetch their
                // respective metadata, which we link to here.
                #( fn #ink_event_metadata_fns () -> ::ink::metadata::EventSpec; )*
            }

            fn main() -> Result<(), std::io::Error> {
                let metadata = unsafe {
                    __ink_generate_metadata(
                        ::ink::prelude::vec![
                            #(
                                #ink_event_metadata_fns ()
                            ),*
                        ]
                    )
                };

                let contents = serde_json::to_string_pretty(&metadata)?;
                print!("{}", contents);
                Ok(())
            }
        )
    }
}

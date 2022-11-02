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
    event_definition_ids: EventDefinitionIds,
}

impl MetadataPackage {
    /// Construct a new [`MetadataPackage`].
    pub fn new(
        contract_package_name: String,
        event_definition_ids: EventDefinitionIds,
    ) -> Self {
        Self {
            event_definition_ids,
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
        let event_definition_ids = &self.event_definition_ids.ids;

        quote::quote!(
            extern crate contract;

            extern "Rust" {
                // Note: The ink! metadata codegen generates an implementation for this function,
                // which is what we end up linking to here.
                fn __ink_generate_metadata(
                    events: ::ink::prelude::vec::Vec<::ink::metadata::EventSpec>
                ) -> ::ink::metadata::InkProject;
            }

            fn main() -> Result<(), std::io::Error> {
                // gather metadata for all `#[ink::event_definition]`s imported by the contract
                let event_definitions = ::ink::prelude::vec![
                    #(
                        <<::ink::reflect::EventDefinitionRegistry as
                            ::ink::reflect::EventDefinition<{ #event_definition_ids }>>::Type as
                                ::ink::metadata::EventMetadata>::event_spec()
                    ),*
                ];

                let metadata = unsafe { __ink_generate_metadata(event_definitions) };

                let contents = serde_json::to_string_pretty(&metadata)?;
                print!("{}", contents);
                Ok(())
            }
        )
    }
}

/// The identifiers of all event definitions imported into a contract.
///
/// These are used to generate function calls to extract metadata from all the events which could
/// be emitted by a contract.
#[derive(Debug)]
pub struct EventDefinitionIds {
    ids: Vec<u128>
}

impl TryFrom<&[u8]> for EventDefinitionIds {
    type Error = anyhow::Error;

    fn try_from(value: &[u8]) -> std::result::Result<Self, Self::Error> {
        let mut cursor = 0;
        let mut buf = [0u8; 16];
        let mut ids = Vec::new();
        buf.copy_from_slice(&value[cursor..cursor + 16]);
        ids.push(u128::from_be_bytes(buf));
        Ok(Self { ids })
    }
}

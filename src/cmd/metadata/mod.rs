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

mod contract;

use cargo_metadata::Metadata;
use crate::{
    util,
    workspace::{ManifestPath, Workspace},
    UnstableFlags, Verbosity,
};
use anyhow::Result;
use contract::{
    Compiler, Contract, ContractMetadata, Language, License, Source, SourceCompiler,
    SourceLanguage, User,
};
use semver::Version;
use std::fs;

const METADATA_FILE: &str = "metadata.json";

/// Executes the metadata generation process
struct GenerateMetadataCommand {
    manifest_path: ManifestPath,
    verbosity: Option<Verbosity>,
    unstable_options: UnstableFlags,
}

impl GenerateMetadataCommand {
    pub fn exec(&self)-> Result<String> {
        util::assert_channel()?;
        println!("  Generating metadata");

        let (cargo_meta, root_package_id) = crate::util::get_cargo_metadata(&self.manifest_path)?;

        let out_path = cargo_meta.target_directory.join(METADATA_FILE);
        let out_path_display = format!("{}", out_path.display());

        let target_dir = cargo_meta.target_directory.clone();

        // build the extended contract project metadata
        let (source_meta, contract_meta, user_meta) = self.extended_metadata(&cargo_meta)?;

        let generate_metadata = |manifest_path: &ManifestPath| -> Result<()> {
            let target_dir_arg = format!("--target-dir={}", target_dir.to_string_lossy());
            let stdout = util::invoke_cargo(
                "run",
                &[
                    "--package",
                    "metadata-gen",
                    &manifest_path.cargo_arg(),
                    &target_dir_arg,
                    "--release",
                    // "--no-default-features", // Breaks builds for MacOS (linker errors), we should investigate this issue asap!
                ],
                self.manifest_path.directory(),
                self.verbosity,
            )?;

            let ink_meta: serde_json::Map<String, serde_json::Value> =
                serde_json::from_slice(&stdout)?;
            let metadata = ContractMetadata::new(source_meta, contract_meta, user_meta, ink_meta);
            let contents = serde_json::to_string_pretty(&metadata)?;
            fs::write(&out_path, contents)?;
            Ok(())
        };

        if self.unstable_options.original_manifest {
            generate_metadata(&self.manifest_path)?;
        } else {
            Workspace::new(&cargo_meta, &root_package_id)?
                .with_root_package_manifest(|manifest| {
                    manifest
                        .with_added_crate_type("rlib")?
                        .with_profile_release_lto(false)?;
                    Ok(())
                })?
                .with_metadata_gen_package()?
                .using_temp(generate_metadata)?;
        }

        Ok(format!(
            "Your metadata file is ready.\nYou can find it here:\n{}",
            out_path_display
        ))
    }

    /// Generate the extended contract project metadata
    fn extended_metadata(&self, cargo_meta: &Metadata) -> Result<(Source, Contract, Option<User>)> {
        // todo: generate these params
        let ink_version = Version::new(2, 1, 0);
        let rust_version = Version::new(1, 41, 0);
        let contract_name = "test".to_string();
        let contract_version = Version::new(0, 0, 0);
        let contract_authors = vec!["author@example.com".to_string()];
        // optional
        let description: Option<String> = None;
        let documentation = None;
        let repository = None;
        let homepage = None;
        let license: Option<License> = None;

        let hash = self.wasm_hash()?;

        let source = {
            let lang = SourceLanguage::new(Language::Ink, ink_version);
            let compiler = SourceCompiler::new(Compiler::RustC, rust_version);
            Source::new(hash, lang, compiler)
        };

        // Required contract fields
        let contract = Contract::new(
            contract_name,
            contract_version,
            contract_authors,
            description,
            documentation,
            repository,
            homepage,
            license,
        );

        let user: Option<User> = None;

        Ok((source, contract, user))
    }

    /// Compile the contract and then hash the resulting wasm
    fn wasm_hash(&self) -> Result<[u8; 32]> {
        let wasm_path = super::execute_build(self.manifest_path.clone(), self.verbosity, self.unstable_options.clone())?;
        let wasm = fs::read(wasm_path)?;

        use ::blake2::digest::{
            Update as _,
            VariableOutput as _,
        };
        let mut output = [0u8; 32];
        let mut blake2 = blake2::VarBlake2b::new_keyed(&[], 32);
        blake2.update(wasm);
        blake2.finalize_variable(|result| output.copy_from_slice(result));
        Ok(output)
    }
}

/// Generates a file with metadata describing the ABI of the smart-contract.
///
/// It does so by generating and invoking a temporary workspace member.
pub(crate) fn execute_generate_metadata(
    manifest_path: ManifestPath,
    verbosity: Option<Verbosity>,
    unstable_options: UnstableFlags,
) -> Result<String> {
    GenerateMetadataCommand {
        manifest_path,
        verbosity,
        unstable_options,
    }.exec()
}

#[cfg(feature = "test-ci-only")]
#[cfg(test)]
mod tests {
    use crate::{
        cmd::{execute_generate_metadata, execute_new},
        util::tests::with_tmp_dir,
        workspace::ManifestPath,
        UnstableFlags,
    };

    #[test]
    fn generate_metadata() {
        env_logger::try_init().ok();
        with_tmp_dir(|path| {
            execute_new("new_project", Some(path)).expect("new project creation failed");
            let working_dir = path.join("new_project");
            let manifest_path = ManifestPath::new(working_dir.join("Cargo.toml")).unwrap();
            let message = execute_generate_metadata(manifest_path, None, UnstableFlags::default())
                .expect("generate metadata failed");
            println!("{}", message);

            let mut metadata_file = working_dir;
            metadata_file.push("target");
            metadata_file.push("metadata.json");
            assert!(
                metadata_file.exists(),
                format!("Missing metadata file '{}'", metadata_file.display())
            )
        });
    }
}

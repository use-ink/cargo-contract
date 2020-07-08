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

use contract::{
    Compiler,
    ContractMetadata,
    Source,
    SourceCompiler,
    SourceLanguage,
    Language,
    License,
    Contract,
    ContractBuilder,
    User,
};
use crate::{
    util,
    workspace::{ManifestPath, Workspace},
    UnstableFlags, Verbosity,
};
use anyhow::Result;
use serde_json::{Map, Value};
use std::fs;
use semver::Version;

const METADATA_FILE: &str = "metadata.json";

/// Generates a file with metadata describing the ABI of the smart-contract.
///
/// It does so by generating and invoking a temporary workspace member.
pub(crate) fn execute_generate_metadata(
    original_manifest_path: ManifestPath,
    verbosity: Option<Verbosity>,
    unstable_options: UnstableFlags,
) -> Result<String> {
    util::assert_channel()?;
    println!("  Generating metadata");

    let (metadata, root_package_id) = crate::util::get_cargo_metadata(&original_manifest_path)?;

    let out_path = metadata.target_directory.join(METADATA_FILE);
    let out_path_display = format!("{}", out_path.display());

    let target_dir = metadata.target_directory.clone();

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
            original_manifest_path.directory(),
            verbosity,
        )?;

        let metadata_json: serde_json::Map<String, serde_json::Value> = serde_json::from_slice(&stdout)?;
        let contents = serde_json::to_string_pretty(&metadata_json)?;
        fs::write(&out_path, contents)?;
        Ok(())
    };

    if unstable_options.original_manifest {
        generate_metadata(&original_manifest_path)?;
    } else {
        Workspace::new(&metadata, &root_package_id)?
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

fn construct_metadata(ink_metadata: Map<String, Value>) -> Result<ContractMetadata> {
    // todo: generate these params
    let hash = [0u8; 32];
    let ink_version = Version::new(2, 1, 0);
    let rust_version = Version::new(1, 41, 0);
    let contract_name = "test";
    let contract_version = Version::new(0, 0, 0);
    let contract_authors = vec!["author@example.com"];
    // optional
    let description: Option<&str> = None;
    let documentation = None;
    let repository = None;
    let homepage = None;
    let license = None;

    let source = {
        let lang = SourceLanguage::new(Language::Ink, ink_version);
        let compiler = SourceCompiler::new(Compiler::RustC, rust_version);
        Source::new(hash, lang, compiler)
    };

    // Required contract fields
    let contract = Contract::build()
        .name(contract_name)
        .version(contract_version)
        .authors(contract_authors);

    // Optional fields
    if let Some(description) = description {
        contract.description(description);
    }
    if let Some(documentation) = documentation {
        contract.documentation(documentation);
    }
    if let Some(repository) = repository {
        contract.repository(repository);
    }
    if let Some(homepage) = homepage {
        contract.homepage(homepage);
    }
    if let Some(license) = license {
        contract.license(license);
    }

    let user: Option<User> = None;

    Ok(ContractMetadata::new(source, contract.done(), user, ink_metadata))
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

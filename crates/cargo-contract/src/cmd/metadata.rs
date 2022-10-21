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

use crate::{
    crate_metadata::CrateMetadata,
    maybe_println,
    util,
    workspace::{
        ManifestPath,
        Workspace,
    },
    Network,
    UnstableFlags,
    Verbosity,
};

use anyhow::Result;
use blake2::digest::{
    consts::U32,
    Digest as _,
};
use colored::Colorize;
use contract_metadata::{
    CodeHash,
    Compiler,
    Contract,
    ContractMetadata,
    Language,
    Source,
    SourceCompiler,
    SourceLanguage,
    SourceWasm,
    User,
};
use semver::Version;
use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};
use url::Url;

const METADATA_FILE: &str = "metadata.json";
const INK_EVENT_METADATA_SECTION_PREFIX: &str = "__ink_event_metadata_";

/// Metadata generation result.
#[derive(serde::Serialize)]
pub struct MetadataResult {
    /// Path to the resulting metadata file.
    pub dest_metadata: PathBuf,
    /// Path to the bundled file.
    pub dest_bundle: PathBuf,
}

/// Result of generating the extended contract project metadata
struct ExtendedMetadataResult {
    source: Source,
    contract: Contract,
    user: Option<User>,
}

/// Generates a file with metadata describing the ABI of the smart contract.
///
/// It does so by generating and invoking a temporary workspace member.
pub(crate) fn execute(
    crate_metadata: &CrateMetadata,
    final_contract_wasm: &Path,
    network: Network,
    verbosity: Verbosity,
    total_steps: usize,
    unstable_options: &UnstableFlags,
) -> Result<MetadataResult> {
    let target_directory = crate_metadata.target_directory.clone();
    let out_path_metadata = target_directory.join(METADATA_FILE);

    let fname_bundle = format!("{}.contract", crate_metadata.contract_artifact_name);
    let out_path_bundle = target_directory.join(fname_bundle);

    // build the extended contract project metadata
    let ExtendedMetadataResult {
        source,
        contract,
        user,
    } = extended_metadata(crate_metadata, final_contract_wasm)?;

    let generate_metadata = |manifest_path: &ManifestPath| -> Result<()> {
        let mut current_progress = 5;
        maybe_println!(
            verbosity,
            " {} {}",
            format!("[{}/{}]", current_progress, total_steps).bold(),
            "Generating metadata".bright_green().bold()
        );
        let target_dir_arg =
            format!("--target-dir={}", target_directory.to_string_lossy());
        let stdout = util::invoke_cargo(
            "run",
            &[
                "--package",
                "metadata-gen",
                &manifest_path.cargo_arg()?,
                &target_dir_arg,
                "--release",
                &network.to_string(),
            ],
            crate_metadata.manifest_path.directory(),
            verbosity,
            vec![],
        )?;

        let ink_meta: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&stdout)?;
        let metadata = ContractMetadata::new(source, contract, user, ink_meta);
        {
            let mut metadata = metadata.clone();
            metadata.remove_source_wasm_attribute();
            let contents = serde_json::to_string_pretty(&metadata)?;
            fs::write(&out_path_metadata, contents)?;
            current_progress += 1;
        }

        maybe_println!(
            verbosity,
            " {} {}",
            format!("[{}/{}]", current_progress, total_steps).bold(),
            "Generating bundle".bright_green().bold()
        );
        let contents = serde_json::to_string(&metadata)?;
        fs::write(&out_path_bundle, contents)?;

        Ok(())
    };

    let module: parity_wasm::elements::Module =
        parity_wasm::deserialize_file(&crate_metadata.original_wasm)?;
    let ink_event_metadata_externs = module
        .custom_sections()
        .filter_map(|section| {
            if section
                .name()
                .starts_with(INK_EVENT_METADATA_SECTION_PREFIX)
            {
                Some(section.name().to_owned())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if unstable_options.original_manifest {
        generate_metadata(&crate_metadata.manifest_path)?;
    } else {
        Workspace::new(&crate_metadata.cargo_meta, &crate_metadata.root_package.id)?
            .with_root_package_manifest(|manifest| {
                manifest
                    .with_added_crate_type("rlib")?
                    .with_profile_release_lto(false)?;
                Ok(())
            })?
            .with_metadata_gen_package(
                crate_metadata.manifest_path.absolute_directory()?,
                ink_event_metadata_externs,
            )?
            .using_temp(generate_metadata)?;
    }

    Ok(MetadataResult {
        dest_metadata: out_path_metadata,
        dest_bundle: out_path_bundle,
    })
}

/// Generate the extended contract project metadata
fn extended_metadata(
    crate_metadata: &CrateMetadata,
    final_contract_wasm: &Path,
) -> Result<ExtendedMetadataResult> {
    let contract_package = &crate_metadata.root_package;
    let ink_version = &crate_metadata.ink_version;
    let rust_version = Version::parse(&rustc_version::version()?.to_string())?;
    let contract_name = contract_package.name.clone();
    let contract_version = Version::parse(&contract_package.version.to_string())?;
    let contract_authors = contract_package.authors.clone();
    // optional
    let description = contract_package.description.clone();
    let documentation = crate_metadata.documentation.clone();
    let repository = contract_package
        .repository
        .as_ref()
        .map(|repo| Url::parse(repo))
        .transpose()?;
    let homepage = crate_metadata.homepage.clone();
    let license = contract_package.license.clone();
    let source = {
        let lang = SourceLanguage::new(Language::Ink, ink_version.clone());
        let compiler = SourceCompiler::new(Compiler::RustC, rust_version);
        let wasm = fs::read(final_contract_wasm)?;
        let hash = blake2_hash(wasm.as_slice());
        Source::new(Some(SourceWasm::new(wasm)), hash, lang, compiler)
    };

    // Required contract fields
    let mut builder = Contract::builder();
    builder
        .name(contract_name)
        .version(contract_version)
        .authors(contract_authors);

    if let Some(description) = description {
        builder.description(description);
    }

    if let Some(documentation) = documentation {
        builder.documentation(documentation);
    }

    if let Some(repository) = repository {
        builder.repository(repository);
    }

    if let Some(homepage) = homepage {
        builder.homepage(homepage);
    }

    if let Some(license) = license {
        builder.license(license);
    }

    let contract = builder.build().map_err(|err| {
        anyhow::anyhow!("Invalid contract metadata builder state: {}", err)
    })?;

    // user defined metadata
    let user = crate_metadata.user.clone().map(User::new);

    Ok(ExtendedMetadataResult {
        source,
        contract,
        user,
    })
}

/// Returns the blake2 hash of the submitted slice.
pub fn blake2_hash(code: &[u8]) -> CodeHash {
    let mut blake2 = blake2::Blake2b::<U32>::new();
    blake2.update(code);
    let result = blake2.finalize();
    CodeHash(result.into())
}

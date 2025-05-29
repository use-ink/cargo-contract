// Copyright (C) Use Ink (UK) Ltd.
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

use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use anyhow::Result;
use colored::Colorize;
use contract_metadata::{
    Compiler,
    Contract,
    ContractMetadata,
    Language,
    Source,
    SourceCompiler,
    SourceContractBinary,
    SourceLanguage,
    User,
};
use ink_metadata::InkProject;
use semver::Version;
use serde::{
    Deserialize,
    Serialize,
};
use url::Url;

use crate::{
    code_hash,
    crate_metadata::CrateMetadata,
    solidity_metadata::{
        self,
        SolidityContractMetadata,
        SolidityMetadataArtifacts,
    },
    util,
    verbose_eprintln,
    workspace::{
        ManifestPath,
        Workspace,
    },
    BuildMode,
    Features,
    Lto,
    Network,
    Profile,
    UnstableFlags,
    Verbosity,
};

/// Artifacts resulting from metadata generation.
#[derive(serde::Serialize, serde::Deserialize)]
pub enum MetadataArtifacts {
    /// Artifacts resulting from ink! metadata generation.
    Ink(InkMetadataArtifacts),
    /// Artifacts resulting from Solidity compatible metadata generation.
    Solidity(SolidityMetadataArtifacts),
}

impl MetadataArtifacts {
    /// Returns true if all metadata files exist.
    pub(crate) fn exists(&self) -> bool {
        match self {
            MetadataArtifacts::Ink(ink_metadata_artifacts) => {
                ink_metadata_artifacts.dest_metadata.exists()
                    && ink_metadata_artifacts.dest_bundle.exists()
            }
            MetadataArtifacts::Solidity(solidity_metadata_artifacts) => {
                solidity_metadata_artifacts.dest_abi.exists()
                    && solidity_metadata_artifacts.dest_metadata.exists()
            }
        }
    }
}

/// Artifacts resulting from ink! metadata generation.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct InkMetadataArtifacts {
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

/// Information about the settings used to build a particular ink! contract.
///
/// Note that this should be an optional part of the metadata since it may not necessarily
/// translate to other languages (e.g. ask!, Solidity).
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct BuildInfo {
    /// The Rust toolchain used to build the contract.
    pub rust_toolchain: String,
    /// The version of `cargo-contract` used to build the contract.
    pub cargo_contract_version: Version,
    /// The type of build that was used when building the contract.
    pub build_mode: BuildMode,
}

impl TryFrom<BuildInfo> for serde_json::Map<String, serde_json::Value> {
    type Error = serde_json::Error;

    fn try_from(build_info: BuildInfo) -> Result<Self, Self::Error> {
        let tmp = serde_json::to_string(&build_info)?;
        serde_json::from_str(&tmp)
    }
}

/// Multi ABI metadata from by ink! codegen.
#[derive(Debug, Serialize, Deserialize)]
pub struct CodegenMetadata {
    ink: Option<InkProject>,
    solidity: Option<ink_metadata::sol::ContractMetadata>,
}

/// Generates a file with metadata describing the ABI of the smart contract.
///
/// It does so by generating and invoking a temporary workspace member.
#[allow(clippy::too_many_arguments)]
pub fn execute(
    crate_metadata: &CrateMetadata,
    final_contract_binary: &Path,
    metadata_artifacts: &MetadataArtifacts,
    features: &Features,
    network: Network,
    verbosity: Verbosity,
    unstable_options: &UnstableFlags,
    build_info: BuildInfo,
) -> Result<()> {
    // build the extended contract project metadata
    let ExtendedMetadataResult {
        source,
        contract,
        user,
    } = extended_metadata(crate_metadata, final_contract_binary, build_info)?;

    let generate_metadata = |manifest_path: &ManifestPath| -> Result<()> {
        verbose_eprintln!(
            verbosity,
            " {} {}",
            "[==]".bold(),
            "Generating metadata".bright_green().bold(),
        );
        let target_dir = crate_metadata
            .target_directory
            .to_string_lossy()
            .to_string();
        let mut args = vec![
            "--package".to_owned(),
            "metadata-gen".to_owned(),
            manifest_path.cargo_arg()?,
            "--target-dir".to_owned(),
            target_dir,
            "--release".to_owned(),
        ];
        network.append_to_args(&mut args);
        features.append_to_args(&mut args);

        #[cfg(windows)]
        let link_dead_code = "";

        #[cfg(not(windows))]
        let link_dead_code = "\x1f-Clink-dead-code";

        let mut abi_cfg = String::new();
        if let Some(abi) = crate_metadata.abi {
            abi_cfg.push('\x1f');
            abi_cfg.push_str(&abi.cargo_encoded_rustflag());
        }

        let cmd = util::cargo_cmd(
            "run",
            args,
            crate_metadata.manifest_path.directory(),
            verbosity,
            vec![(
                "CARGO_ENCODED_RUSTFLAGS",
                Some(format!("--cap-lints=allow{link_dead_code}{abi_cfg}")),
            )],
        );
        let output = cmd.stdout_capture().run()?;
        let codegen_meta: CodegenMetadata = serde_json::from_slice(&output.stdout)?;
        match metadata_artifacts {
            MetadataArtifacts::Ink(ink_metadata_artifacts) => {
                let ink_project = codegen_meta
                    .ink
                    .ok_or_else(|| anyhow::anyhow!("Expected ink! metadata"))?;
                let ink_meta = match serde_json::to_value(&ink_project)? {
                    serde_json::Value::Object(meta) => meta,
                    _ => anyhow::bail!("Expected ink! metadata object"),
                };
                let metadata =
                    ContractMetadata::new(source, contract, None, user, ink_meta);

                write_metadata(ink_metadata_artifacts, metadata, &verbosity, false)?;
            }
            MetadataArtifacts::Solidity(solidity_metadata_artifacts) => {
                let sol_meta = codegen_meta.solidity.ok_or_else(|| {
                    anyhow::anyhow!("Expected Solidity compatibility metadata")
                })?;
                let sol_abi = solidity_metadata::generate_abi(&sol_meta)?;
                let metadata = solidity_metadata::generate_metadata(
                    &sol_meta,
                    sol_abi,
                    source,
                    contract,
                    crate_metadata,
                    None,
                )?;

                write_solidity_metadata(
                    solidity_metadata_artifacts,
                    metadata,
                    &verbosity,
                    false,
                )?;
            }
        }

        Ok(())
    };

    if unstable_options.original_manifest {
        generate_metadata(&crate_metadata.manifest_path)?;
    } else {
        Workspace::new(&crate_metadata.cargo_meta, &crate_metadata.root_package.id)?
            .with_root_package_manifest(|manifest| {
                manifest
                    .with_added_crate_type("rlib")?
                    .with_profile_release_defaults(Profile {
                        lto: Some(Lto::Thin),
                        ..Profile::default()
                    })?
                    .with_merged_workspace_dependencies(crate_metadata)?
                    .with_empty_workspace();
                Ok(())
            })?
            .with_metadata_gen_package()?
            .using_temp(generate_metadata)?;
    }

    Ok(())
}

pub fn write_metadata(
    metadata_artifacts: &InkMetadataArtifacts,
    metadata: ContractMetadata,
    verbosity: &Verbosity,
    overwrite: bool,
) -> Result<()> {
    {
        let mut metadata = metadata.clone();
        metadata.remove_source_contract_binary_attribute();
        let contents = serde_json::to_string_pretty(&metadata)?;
        fs::write(&metadata_artifacts.dest_metadata, contents)?;
    }

    if overwrite {
        verbose_eprintln!(
            verbosity,
            " {} {}",
            "[==]".bold(),
            "Updating paths".bright_cyan().bold()
        );
    } else {
        verbose_eprintln!(
            verbosity,
            " {} {}",
            "[==]".bold(),
            "Generating bundle".bright_green().bold()
        );
    }
    let contents = serde_json::to_string(&metadata)?;
    fs::write(&metadata_artifacts.dest_bundle, contents)?;

    Ok(())
}

/// Writes Solidity compatible ABI and metadata files.
pub fn write_solidity_metadata(
    metadata_artifacts: &SolidityMetadataArtifacts,
    metadata: SolidityContractMetadata,
    verbosity: &Verbosity,
    overwrite: bool,
) -> Result<()> {
    if overwrite {
        verbose_eprintln!(
            verbosity,
            " {} {}",
            "[==]".bold(),
            "Updating Solidity compatible metadata".bright_cyan().bold()
        );
    } else {
        verbose_eprintln!(
            verbosity,
            " {} {}",
            "[==]".bold(),
            "Generating Solidity compatible metadata"
                .bright_green()
                .bold()
        );
    }

    // Writes Solidity ABI file.
    solidity_metadata::write_abi(&metadata.output.abi, &metadata_artifacts.dest_abi)?;

    // Writes Solidity Metadata file.
    solidity_metadata::write_metadata(&metadata, &metadata_artifacts.dest_metadata)?;

    Ok(())
}

/// Generate the extended contract project metadata
fn extended_metadata(
    crate_metadata: &CrateMetadata,
    final_contract_binary: &Path,
    build_info: BuildInfo,
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
        let contract_binary = fs::read(final_contract_binary)?;
        let hash = code_hash(contract_binary.as_slice());
        Source::new(
            Some(SourceContractBinary::new(contract_binary)),
            hash.into(),
            lang,
            compiler,
            Some(build_info.try_into()?),
        )
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

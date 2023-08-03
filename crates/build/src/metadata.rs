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
    code_hash,
    crate_metadata::CrateMetadata,
    util,
    verbose_eprintln,
    workspace::{
        ManifestPath,
        Workspace,
    },
    BuildMode,
    BuildSteps,
    Features,
    Lto,
    Network,
    OptimizationPasses,
    Profile,
    UnstableFlags,
    Verbosity,
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

/// Artifacts resulting from metadata generation.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MetadataArtifacts {
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
/// translate to other languages (e.g ask!, Solidity).
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct BuildInfo {
    /// The Rust toolchain used to build the contract.
    pub rust_toolchain: String,
    /// The version of `cargo-contract` used to build the contract.
    pub cargo_contract_version: Version,
    /// The type of build that was used when building the contract.
    pub build_mode: BuildMode,
    /// Information about the `wasm-opt` optimization settings.
    pub wasm_opt_settings: WasmOptSettings,
}

impl TryFrom<BuildInfo> for serde_json::Map<String, serde_json::Value> {
    type Error = serde_json::Error;

    fn try_from(build_info: BuildInfo) -> Result<Self, Self::Error> {
        let tmp = serde_json::to_string(&build_info)?;
        serde_json::from_str(&tmp)
    }
}

/// Settings used when optimizing the Wasm binary using Binaryen's `wasm-opt`.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct WasmOptSettings {
    /// The level of optimization used during the `wasm-opt` run.
    pub optimization_passes: OptimizationPasses,
    /// Whether or not the Wasm name section should be kept.
    pub keep_debug_symbols: bool,
}

/// Generates a file with metadata describing the ABI of the smart contract.
///
/// It does so by generating and invoking a temporary workspace member.
#[allow(clippy::too_many_arguments)]
pub fn execute(
    crate_metadata: &CrateMetadata,
    final_contract_wasm: &Path,
    metadata_artifacts: &MetadataArtifacts,
    features: &Features,
    network: Network,
    verbosity: Verbosity,
    mut build_steps: BuildSteps,
    unstable_options: &UnstableFlags,
    build_info: BuildInfo,
) -> Result<()> {
    // build the extended contract project metadata
    let ExtendedMetadataResult {
        source,
        contract,
        user,
    } = extended_metadata(crate_metadata, final_contract_wasm, build_info)?;

    let generate_metadata = |manifest_path: &ManifestPath| -> Result<()> {
        verbose_eprintln!(
            verbosity,
            " {} {}",
            format!("{build_steps}").bold(),
            "Generating metadata".bright_green().bold()
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

        let cmd = util::cargo_cmd(
            "run",
            args,
            crate_metadata.manifest_path.directory(),
            verbosity,
            vec![],
        );
        let output = cmd.stdout_capture().run()?;

        let ink_meta: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&output.stdout)?;
        let metadata = ContractMetadata::new(source, contract, None, user, ink_meta);

        write_metadata(
            metadata_artifacts,
            metadata,
            &mut build_steps,
            &verbosity,
            false,
        )?;

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
                    .with_empty_workspace();
                Ok(())
            })?
            .with_metadata_gen_package()?
            .using_temp(generate_metadata)?;
    }

    Ok(())
}

pub fn write_metadata(
    metadata_artifacts: &MetadataArtifacts,
    metadata: ContractMetadata,
    build_steps: &mut BuildSteps,
    verbosity: &Verbosity,
    overwrite: bool,
) -> Result<()> {
    {
        let mut metadata = metadata.clone();
        metadata.remove_source_wasm_attribute();
        let contents = serde_json::to_string_pretty(&metadata)?;
        fs::write(&metadata_artifacts.dest_metadata, contents)?;
        build_steps.increment_current();
    }

    if overwrite {
        verbose_eprintln!(
            verbosity,
            " {} {}",
            format!("{build_steps}").bold(),
            "Updating paths".bright_cyan().bold()
        );
    } else {
        verbose_eprintln!(
            verbosity,
            " {} {}",
            format!("{build_steps}").bold(),
            "Generating bundle".bright_green().bold()
        );
    }
    let contents = serde_json::to_string(&metadata)?;
    fs::write(&metadata_artifacts.dest_bundle, contents)?;

    Ok(())
}

/// Generate the extended contract project metadata
fn extended_metadata(
    crate_metadata: &CrateMetadata,
    final_contract_wasm: &Path,
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
        let wasm = fs::read(final_contract_wasm)?;
        let hash = code_hash(wasm.as_slice());
        Source::new(
            Some(SourceWasm::new(wasm)),
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

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

use crate::{
    ManifestPath,
    Target,
};
use anyhow::{
    Context,
    Result,
};
use cargo_metadata::{
    Metadata as CargoMetadata,
    MetadataCommand,
    Package,
    TargetKind,
};
use semver::Version;
use serde_json::{
    Map,
    Value,
};
use std::{
    fs,
    path::PathBuf,
};
use toml::value;
use url::Url;

/// Relevant metadata obtained from Cargo.toml.
#[derive(Debug)]
pub struct CrateMetadata {
    pub manifest_path: ManifestPath,
    pub cargo_meta: cargo_metadata::Metadata,
    pub contract_artifact_name: String,
    pub root_package: Package,
    pub original_code: PathBuf,
    pub dest_code: PathBuf,
    pub ink_version: Version,
    pub documentation: Option<Url>,
    pub homepage: Option<Url>,
    pub user: Option<Map<String, Value>>,
    pub target_directory: PathBuf,
    pub target_file_path: PathBuf,
}

impl CrateMetadata {
    /// Attempt to construct [`CrateMetadata`] from the given manifest path.
    pub fn from_manifest_path(
        manifest_path: Option<&PathBuf>,
        target: Target,
    ) -> Result<Self> {
        let manifest_path = ManifestPath::try_from(manifest_path)?;
        Self::collect(&manifest_path, target)
    }

    /// Parses the contract manifest and returns relevant metadata.
    pub fn collect(manifest_path: &ManifestPath, target: Target) -> Result<Self> {
        let (metadata, root_package) = get_cargo_metadata(manifest_path)?;
        let mut target_directory = metadata.target_directory.as_path().join("ink");

        // Normalize the final contract artifact name.
        let contract_artifact_name = root_package.name.replace('-', "_");

        if let Some(lib_name) = &root_package
            .targets
            .iter()
            .find(|target| target.kind.iter().any(|f| *f == TargetKind::Lib))
        {
            if lib_name.name != root_package.name {
                // warn user if they still specify a lib name different from the
                // package name
                use colored::Colorize;
                eprintln!(
                    "{} the `name` field in the `[lib]` section of the `Cargo.toml`, \
                    is no longer used for the name of generated contract artifacts. \
                    The package name is used instead. Remove the `[lib] name` to \
                    stop this warning.",
                    "warning:".yellow().bold(),
                );
            }
        }

        let absolute_manifest_path = manifest_path.absolute_directory()?;
        let absolute_workspace_root = metadata.workspace_root.canonicalize()?;
        if absolute_manifest_path != absolute_workspace_root {
            // If the contract is a package in a workspace, we use the package name
            // as the name of the sub-folder where we put the `.contract` bundle.
            target_directory = target_directory.join(contract_artifact_name.clone());
        }

        // {target_dir}/{target}/release/{contract_artifact_name}.{extension}
        let mut original_code = target_directory.clone();
        original_code.push(target.llvm_target_alias());
        original_code.push("release");
        original_code.push(root_package.name.clone());
        original_code.set_extension(target.source_extension());

        // {target_dir}/{contract_artifact_name}.code
        let mut dest_code = target_directory.clone();
        dest_code.push(contract_artifact_name.clone());
        dest_code.set_extension(target.dest_extension());

        let ink_version = metadata
            .packages
            .iter()
            .find_map(|package| {
                if package.name == "ink" || package.name == "ink_lang" {
                    Some(
                        Version::parse(&package.version.to_string())
                            .expect("Invalid ink crate version string"),
                    )
                } else {
                    None
                }
            })
            .ok_or_else(|| anyhow::anyhow!("No 'ink' dependency found"))?;

        let ExtraMetadata {
            documentation,
            homepage,
            user,
        } = get_cargo_toml_metadata(manifest_path)?;

        let crate_metadata = CrateMetadata {
            manifest_path: manifest_path.clone(),
            cargo_meta: metadata,
            root_package,
            contract_artifact_name,
            original_code: original_code.into(),
            dest_code: dest_code.into(),
            ink_version,
            documentation,
            homepage,
            user,
            target_file_path: target_directory.join(".target").into(),
            target_directory: target_directory.into(),
        };
        Ok(crate_metadata)
    }

    /// Get the path of the contract metadata file
    pub fn metadata_path(&self) -> PathBuf {
        let metadata_file = format!("{}.json", self.contract_artifact_name);
        self.target_directory.join(metadata_file)
    }

    /// Get the path of the contract bundle, containing metadata + code.
    pub fn contract_bundle_path(&self) -> PathBuf {
        let target_directory = self.target_directory.clone();
        let fname_bundle = format!("{}.contract", self.contract_artifact_name);
        target_directory.join(fname_bundle)
    }

    /// Returns `true` if `ink_e2e` is a dependency of the project.
    pub fn depends_on_ink_e2e(&self) -> bool {
        let (metadata, _root_package) = get_cargo_metadata(&self.manifest_path).unwrap();
        metadata
            .packages
            .iter()
            .any(|package| package.name == "ink_e2e")
    }
}

/// Get the result of `cargo metadata`, together with the root package id.
fn get_cargo_metadata(manifest_path: &ManifestPath) -> Result<(CargoMetadata, Package)> {
    tracing::debug!(
        "Fetching cargo metadata for {}",
        manifest_path.as_ref().to_string_lossy()
    );
    let mut cmd = MetadataCommand::new();
    let metadata = cmd
        .manifest_path(manifest_path.as_ref())
        .exec()
        .with_context(|| {
            format!(
                "Error invoking `cargo metadata` for {}",
                manifest_path.as_ref().display()
            )
        })?;
    let root_package_id = metadata
        .resolve
        .as_ref()
        .and_then(|resolve| resolve.root.as_ref())
        .context("Cannot infer the root project id")?
        .clone();
    // Find the root package by id in the list of packages. It is logical error if the
    // root package is not found in the list.
    let root_package = metadata
        .packages
        .iter()
        .find(|package| package.id == root_package_id)
        .expect("The package is not found in the `cargo metadata` output")
        .clone();
    Ok((metadata, root_package))
}

/// Extra metadata not available via `cargo metadata`.
struct ExtraMetadata {
    documentation: Option<Url>,
    homepage: Option<Url>,
    user: Option<Map<String, Value>>,
}

/// Read extra metadata not available via `cargo metadata` directly from `Cargo.toml`
fn get_cargo_toml_metadata(manifest_path: &ManifestPath) -> Result<ExtraMetadata> {
    let toml = fs::read_to_string(manifest_path)?;
    let toml: value::Table = toml::from_str(&toml)?;

    let get_url = |field_name| -> Result<Option<Url>> {
        toml.get("package")
            .ok_or_else(|| anyhow::anyhow!("package section not found"))?
            .get(field_name)
            .and_then(|v| v.as_str())
            .map(Url::parse)
            .transpose()
            .context(format!("{field_name} should be a valid URL"))
            .map_err(Into::into)
    };

    let documentation = get_url("documentation")?;
    let homepage = get_url("homepage")?;

    let user = toml
        .get("package")
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.get("contract"))
        .and_then(|v| v.get("user"))
        .and_then(|v| v.as_table())
        .map(|v| {
            // convert user defined section from toml to json
            serde_json::to_string(v).and_then(|json| serde_json::from_str(&json))
        })
        .transpose()?;

    Ok(ExtraMetadata {
        documentation,
        homepage,
        user,
    })
}

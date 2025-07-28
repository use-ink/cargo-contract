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

use crate::{
    Abi,
    BuildMode,
    ManifestPath,
    Target,
};

/// Relevant metadata obtained from `Cargo.toml`.
#[derive(Debug)]
pub struct CrateMetadata {
    pub manifest_path: ManifestPath,
    pub cargo_meta: cargo_metadata::Metadata,
    pub contract_artifact_name: String,
    pub root_package: Package,
    pub original_code: PathBuf,
    pub dest_binary: PathBuf,
    pub ink_version: Version,
    pub abi: Option<Abi>,
    pub documentation: Option<Url>,
    pub homepage: Option<Url>,
    pub user: Option<Map<String, Value>>,
    /// Directory for intermediate build artifacts.
    ///
    /// Analog to `--target-dir` for cargo.
    pub target_directory: PathBuf,
    /// Directory for final build artifacts.
    ///
    /// Analog to the unstable `--artifact-dir` for cargo.
    ///
    /// Ref: <https://doc.rust-lang.org/cargo/commands/cargo-build.html#output-options>
    pub artifact_directory: PathBuf,
    pub target_file_path: PathBuf,
    pub metadata_spec_path: PathBuf,
}

impl CrateMetadata {
    /// Attempt to construct [`CrateMetadata`] from the given manifest path.
    pub fn from_manifest_path(manifest_path: Option<&PathBuf>) -> Result<Self> {
        let manifest_path = ManifestPath::try_from(manifest_path)?;
        Self::collect(&manifest_path)
    }

    /// Parses the contract manifest and returns relevant metadata.
    pub fn collect(manifest_path: &ManifestPath) -> Result<Self> {
        Self::collect_with_target_dir(manifest_path, None, &BuildMode::Release)
    }

    /// Parses the contract manifest and returns relevant metadata.
    pub fn collect_with_target_dir(
        manifest_path: &ManifestPath,
        target_dir: Option<PathBuf>,
        build_mode: &BuildMode,
    ) -> Result<Self> {
        let (metadata, root_package) = get_cargo_metadata(manifest_path)?;
        let mut target_directory = target_dir
            .as_deref()
            .unwrap_or_else(|| metadata.target_directory.as_std_path())
            .join("ink");

        // Normalize the final contract artifact name.
        let contract_artifact_name = root_package.name.replace('-', "_");

        // Retrieves ABI from package metadata (if specified).
        let abi = package_abi(&root_package).transpose()?;

        if let Some(lib_name) = &root_package
            .targets
            .iter()
            .find(|target| target.kind.contains(&TargetKind::Lib))
        {
            // Warn user if they still specify a lib name different from the
            // package name.
            // NOTE: If no lib name is specified, cargo "normalizes" the package name
            // and auto inserts it as the lib name. So we need to normalize the package
            // name before making the comparison.
            // Ref: <https://github.com/rust-lang/cargo/blob/3c5bb555caf3fad02927fcfd790ee525da17ce5a/src/cargo/util/toml/targets.rs#L177-L178>
            let expected_lib_name = root_package.name.replace("-", "_");
            if lib_name.name != expected_lib_name {
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
        // Allows the final build artifacts (e.g. contract binary, metadata e.t.c) to
        // be placed in a separate directory from the "target" directory used for
        // intermediate build artifacts. This is also similar to `cargo`'s
        // currently unstable `--artifact-dir`, but it's only used internally
        // (at the moment).
        // Ref: <https://doc.rust-lang.org/cargo/commands/cargo-build.html#output-options>
        let mut artifact_directory = target_directory.clone();
        if absolute_manifest_path != absolute_workspace_root {
            // If the contract is a package in a workspace, we use the package name
            // as the name of the sub-folder where we put the `.contract` bundle.
            artifact_directory = artifact_directory.join(contract_artifact_name.clone());
        }

        // Adds ABI sub-folders to target directory for intermediate build artifacts.
        // This is necessary because the ABI is passed as a `cfg` flag,
        // and this ensures that `cargo` will recompile all packages (including proc
        // macros) for current ABI (similar to how it handles profiles and target
        // triples).
        target_directory.push("abi");
        target_directory.push(abi.unwrap_or_default().as_ref());

        // {target_dir}/{target}/release/{contract_artifact_name}.{extension}
        let mut original_code = target_directory.clone();
        original_code.push(Target::llvm_target_alias());
        if build_mode == &BuildMode::Debug {
            original_code.push("debug");
        } else {
            original_code.push("release");
        }
        original_code.push(root_package.name.as_str());
        original_code.set_extension(Target::source_extension());

        // {target_dir}/{contract_artifact_name}.code
        let mut dest_code = artifact_directory.clone();
        dest_code.push(contract_artifact_name.clone());
        dest_code.set_extension(Target::dest_extension());

        let ink_version = metadata
            .packages
            .iter()
            .find_map(|package| {
                if package.name.as_str() == "ink" || package.name.as_str() == "ink_lang" {
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
            original_code,
            dest_binary: dest_code,
            ink_version,
            abi,
            documentation,
            homepage,
            user,
            target_file_path: artifact_directory.join(".target"),
            metadata_spec_path: artifact_directory.join(".metadata_spec"),
            target_directory,
            artifact_directory,
        };
        Ok(crate_metadata)
    }

    /// Get the path of the contract metadata file
    pub fn metadata_path(&self) -> PathBuf {
        let metadata_file = format!("{}.json", self.contract_artifact_name);
        self.artifact_directory.join(metadata_file)
    }

    /// Get the path of the contract bundle, containing metadata + code.
    pub fn contract_bundle_path(&self) -> PathBuf {
        let artifact_directory = self.artifact_directory.clone();
        let fname_bundle = format!("{}.contract", self.contract_artifact_name);
        artifact_directory.join(fname_bundle)
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

/// Returns ABI specified (if any) for the package (i.e. via
/// `package.metadata.ink-lang.abi`).
fn package_abi(package: &Package) -> Option<Result<Abi>> {
    let abi_str = package.metadata.get("ink-lang")?.get("abi")?.as_str()?;
    let abi = match abi_str {
        "ink" => Abi::Ink,
        "sol" => Abi::Solidity,
        "all" => Abi::All,
        _ => return Some(Err(anyhow::anyhow!("Unknown ABI: {abi_str}"))),
    };

    Some(Ok(abi))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        get_cargo_metadata,
        package_abi,
    };
    use crate::{
        new_contract_project,
        util::tests::with_tmp_dir,
        Abi,
        ManifestPath,
    };

    #[test]
    fn valid_package_abi_works() {
        fn test_project_with_abi(abi: Abi) {
            with_tmp_dir(|path| {
                let name = "project_with_valid_abi";
                let dir = path.join(name);
                fs::create_dir_all(&dir).unwrap();
                let result = new_contract_project(name, Some(path), Some(abi));
                assert!(result.is_ok(), "Should succeed");

                let manifest_path = ManifestPath::new(dir.join("Cargo.toml")).unwrap();
                let (_, root_package) = get_cargo_metadata(&manifest_path).unwrap();
                let parsed_abi = package_abi(&root_package)
                    .expect("Expected an ABI declaration")
                    .expect("Expected a valid ABI");
                assert_eq!(parsed_abi, abi);

                Ok(())
            });
        }

        test_project_with_abi(Abi::Ink);
        test_project_with_abi(Abi::Solidity);
        test_project_with_abi(Abi::All);
    }

    #[test]
    fn missing_package_abi_works() {
        with_tmp_dir(|path| {
            let name = "project_with_no_abi";
            let dir = path.join(name);
            fs::create_dir_all(&dir).unwrap();
            let result = new_contract_project(name, Some(path), None);
            assert!(result.is_ok(), "Should succeed");

            let cargo_toml = dir.join("Cargo.toml");
            let mut manifest_content = fs::read_to_string(&cargo_toml).unwrap();
            manifest_content = manifest_content
                .replace("[package.metadata.ink-lang]\nabi = \"ink\"", "");
            let result = fs::write(&cargo_toml, manifest_content);
            assert!(result.is_ok(), "Should succeed");

            let manifest_path = ManifestPath::new(cargo_toml).unwrap();
            let (_, root_package) = get_cargo_metadata(&manifest_path).unwrap();
            let parsed_abi = package_abi(&root_package);
            assert!(parsed_abi.is_none(), "Should be None");

            Ok(())
        });
    }

    #[test]
    fn invalid_package_abi_fails() {
        with_tmp_dir(|path| {
            let name = "project_with_invalid_abi";
            let dir = path.join(name);
            fs::create_dir_all(&dir).unwrap();
            let result = new_contract_project(name, Some(path), None);
            assert!(result.is_ok(), "Should succeed");

            let cargo_toml = dir.join("Cargo.toml");
            let mut manifest_content = fs::read_to_string(&cargo_toml).unwrap();
            manifest_content =
                manifest_content.replace("abi = \"ink\"", "abi = \"move\"");
            let result = fs::write(&cargo_toml, manifest_content);
            assert!(result.is_ok(), "Should succeed");

            let manifest_path = ManifestPath::new(cargo_toml).unwrap();
            let (_, root_package) = get_cargo_metadata(&manifest_path).unwrap();
            let parsed_abi =
                package_abi(&root_package).expect("Expected an ABI declaration");
            assert!(parsed_abi.is_err(), "Should be Err");
            assert!(parsed_abi.unwrap_err().to_string().contains("Unknown ABI"));

            Ok(())
        });
    }
}

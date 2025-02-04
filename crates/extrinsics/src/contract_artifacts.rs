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

use super::{
    ContractBinary,
    ContractMessageTranscoder,
    ContractMetadata,
    CrateMetadata,
};
use anyhow::{
    Context,
    Result,
};
use colored::Colorize;
use ink_metadata::InkProject;
use std::path::{
    Path,
    PathBuf,
};

/// Contract artifacts for use with extrinsic commands.
#[derive(Debug)]
pub struct ContractArtifacts {
    /// The original artifact path
    artifacts_path: PathBuf,
    /// The expected path of the file containing the contract metadata.
    metadata_path: PathBuf,
    /// The deserialized contract metadata if the expected metadata file exists.
    metadata: Option<ContractMetadata>,
    /// The contract binary if available.
    pub contract_binary: Option<ContractBinary>,
}

impl ContractArtifacts {
    /// Load contract artifacts.
    pub fn from_manifest_or_file(
        manifest_path: Option<&PathBuf>,
        file: Option<&PathBuf>,
    ) -> Result<ContractArtifacts> {
        let artifact_path = match (manifest_path, file) {
            (manifest_path, None) => {
                let crate_metadata = CrateMetadata::from_manifest_path(manifest_path)?;

                if crate_metadata.contract_bundle_path().exists() {
                    crate_metadata.contract_bundle_path()
                } else if crate_metadata.metadata_path().exists() {
                    crate_metadata.metadata_path()
                } else {
                    anyhow::bail!(
                        "Failed to find any contract artifacts in target directory. \n\
                        Run `cargo contract build --release` to generate the artifacts."
                    )
                }
            }
            (None, Some(artifact_file)) => artifact_file.clone(),
            (Some(_), Some(_)) => {
                anyhow::bail!("conflicting options: --manifest-path and --file")
            }
        };
        Self::from_artifact_path(artifact_path.as_path())
    }

    /// Given a contract artifact path, load the contract code and metadata where
    /// possible.
    fn from_artifact_path(path: &Path) -> Result<Self> {
        tracing::debug!("Loading contracts artifacts from `{}`", path.display());
        let (metadata_path, metadata, code) =
            match path.extension().and_then(|ext| ext.to_str()) {
                Some("contract") | Some("json") => {
                    let metadata = ContractMetadata::load(path)?;
                    let code = metadata.clone().source.contract_binary.map(|binary| ContractBinary(binary.0));
                    (PathBuf::from(path), Some(metadata), code)
                }
                Some("polkavm") => {
                    let file_name = path.file_stem()
                        .context("PolkaVM bundle file has unreadable name")?
                        .to_str()
                        .context("Error parsing filename string")?;
                    let code = Some(ContractBinary(std::fs::read(path)?));
                    let dir = path.parent().map_or_else(PathBuf::new, PathBuf::from);
                    let metadata_path = dir.join(format!("{file_name}.json"));
                    if !metadata_path.exists() {
                        (metadata_path, None, code)
                    } else {
                        let metadata = ContractMetadata::load(&metadata_path)?;
                        (metadata_path, Some(metadata), code)
                    }
                }
                Some(ext) => anyhow::bail!(
                    "Invalid artifact extension {ext}, expected `.contract`, `.json` or `.polkavm`"
                ),
                None => {
                    anyhow::bail!(
                        "Artifact path has no extension, expected `.contract`, `.json`, or `.polkavm`"
                    )
                }
            };

        if let Some(contract_metadata) = metadata.as_ref() {
            if let Err(e) = contract_metadata.check_ink_compatibility() {
                eprintln!("{} {}", "warning:".yellow().bold(), e.to_string().bold());
            }
        }
        Ok(Self {
            artifacts_path: path.into(),
            metadata_path,
            metadata,
            contract_binary: code,
        })
    }

    /// Get the path of the artifact file used to load the artifacts.
    pub fn artifact_path(&self) -> &Path {
        self.artifacts_path.as_path()
    }

    /// Get contract metadata, if available.
    ///
    /// ## Errors
    /// - No contract metadata could be found.
    /// - Invalid contract metadata.
    pub fn metadata(&self) -> Result<ContractMetadata> {
        self.metadata.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "No contract metadata found. Expected file {}",
                self.metadata_path.as_path().display()
            )
        })
    }

    /// Get the deserialized [`InkProject`] metadata.
    ///
    /// ## Errors
    /// - No contract metadata could be found.
    /// - Invalid contract metadata.
    pub fn ink_project_metadata(&self) -> Result<InkProject> {
        let metadata = self.metadata()?;
        let ink_project = serde_json::from_value(serde_json::Value::Object(metadata.abi))
            .context(
                "Failed to deserialize ink project metadata from contract metadata",
            )?;
        Ok(ink_project)
    }

    /// Get the code hash from the contract metadata.
    pub fn code_hash(&self) -> Result<[u8; 32]> {
        let metadata = self.metadata()?;
        Ok(metadata.source.hash.0)
    }

    /// Construct a [`ContractMessageTranscoder`] from contract metadata.
    pub fn contract_transcoder(&self) -> Result<ContractMessageTranscoder> {
        let metadata = self.metadata()?;
        ContractMessageTranscoder::try_from(metadata)
            .context("Failed to deserialize ink project metadata from contract metadata")
    }

    /// Returns `true` if the image is verifiable.
    ///
    /// If the metadata cannot be extracted we assume that it can't be verified.
    pub fn is_verifiable(&self) -> bool {
        match self.metadata() {
            Ok(m) => m.image.is_some(),
            Err(_) => false,
        }
    }
}

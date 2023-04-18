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

mod manifest;
mod metadata;
mod profile;

#[doc(inline)]
pub use self::{
    manifest::{
        Manifest,
        ManifestPath,
    },
    profile::Profile,
};

use anyhow::Result;
use cargo_metadata::{
    Metadata as CargoMetadata,
    Package,
    PackageId,
};

use std::path::{
    Path,
    PathBuf,
};

/// Make a copy of a cargo workspace, maintaining only the directory structure and manifest
/// files. Relative paths to source files and non-workspace dependencies are rewritten to absolute
/// paths to the original locations.
///
/// This allows custom amendments to be made to the manifest files without editing the originals
/// directly.
pub struct Workspace {
    workspace_root: PathBuf,
    root_package: Package,
    root_manifest: Manifest,
}

impl Workspace {
    /// Create a new Workspace from the supplied cargo metadata.
    pub fn new(metadata: &CargoMetadata, root_package: &PackageId) -> Result<Self> {
        let root_package = metadata
            .packages
            .iter()
            .find(|p| p.id == *root_package)
            .ok_or_else(|| {
                anyhow::anyhow!("The root package should be a workspace member")
            })?;

        let manifest_path = ManifestPath::new(&root_package.manifest_path)?;
        let root_manifest = Manifest::new(manifest_path)?;

        Ok(Workspace {
            workspace_root: metadata.workspace_root.clone().into(),
            root_package: root_package.clone(),
            root_manifest,
        })
    }

    /// Amend the root package manifest using the supplied function.
    ///
    /// # Note
    ///
    /// The root package is the current workspace package being built, not to be confused with
    /// the workspace root (where the top level workspace `Cargo.toml` is defined).
    pub fn with_root_package_manifest<F>(&mut self, f: F) -> Result<&mut Self>
    where
        F: FnOnce(&mut Manifest) -> Result<()>,
    {
        f(&mut self.root_manifest)?;
        Ok(self)
    }

    /// Generates a package to invoke for generating contract metadata.
    ///
    /// The contract metadata will be generated for the package found at `package_path`.
    pub(super) fn with_metadata_gen_package(&mut self) -> Result<&mut Self> {
        self.root_manifest.with_metadata_package()?;
        Ok(self)
    }

    /// Writes the amended manifests to the `target` directory, retaining the workspace directory
    /// structure, but only with the `Cargo.toml` files.
    ///
    /// Relative paths will be rewritten to absolute paths from the original workspace root, except
    /// intra-workspace relative dependency paths which will be preserved.
    ///
    /// Returns the paths of the new manifests.
    pub fn write<P: AsRef<Path>>(&mut self, target: P) -> Result<ManifestPath> {
        // replace the original workspace root with the temporary directory
        let mut new_path: PathBuf = target.as_ref().into();
        new_path.push(
            self.root_package
                .manifest_path
                .strip_prefix(&self.workspace_root)?,
        );
        let new_manifest = ManifestPath::new(new_path)?;

        // tracing::info!("Rewriting manifest {} to {}", self.root_manifest.manifest_path, new_manifest.display());

        self.root_manifest.rewrite_relative_paths()?;
        self.root_manifest.write(&new_manifest)?;

        Ok(new_manifest)
    }

    /// Copy the workspace with amended manifest files to a temporary directory, executing the
    /// supplied function with the root manifest path before the directory is cleaned up.
    pub fn using_temp<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&ManifestPath) -> Result<()>,
    {
        let tmp_dir = tempfile::Builder::new()
            .prefix("cargo-contract_")
            .tempdir()?;
        tracing::debug!("Using temp workspace at '{}'", tmp_dir.path().display());
        let tmp_root_manifest_path = self.write(&tmp_dir)?;

        // copy the `Cargo.lock` file
        let src_lockfile = self.workspace_root.clone().join("Cargo.lock");
        let dest_lockfile = tmp_dir.path().join("Cargo.lock");
        if src_lockfile.exists() {
            tracing::debug!(
                "Copying '{}' to ' '{}'",
                src_lockfile.display(),
                dest_lockfile.display()
            );
            std::fs::copy(src_lockfile, dest_lockfile)?;
        }

        f(&tmp_root_manifest_path)
    }
}

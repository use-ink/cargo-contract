// Copyright 2018-2021 Parity Technologies (UK) Ltd.
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
    manifest::{Manifest, ManifestPath},
    profile::Profile,
};

use anyhow::Result;
use cargo_metadata::{Metadata as CargoMetadata, Package, PackageId};

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Make a copy of a cargo workspace, maintaining only the directory structure and manifest
/// files. Relative paths to source files and non-workspace dependencies are rewritten to absolute
/// paths to the original locations.
///
/// This allows custom amendments to be made to the manifest files without editing the originals
/// directly.
pub struct Workspace {
    workspace_root: PathBuf,
    root_package: PackageId,
    members: HashMap<PackageId, (Package, Manifest)>,
}

impl Workspace {
    /// Create a new Workspace from the supplied cargo metadata.
    pub fn new(metadata: &CargoMetadata, root_package: &PackageId) -> Result<Self> {
        let member_manifest = |package_id: &PackageId| -> Result<(PackageId, (Package, Manifest))> {
            let package = metadata
                .packages
                .iter()
                .find(|p| p.id == *package_id)
                .unwrap_or_else(|| {
                    panic!(
                        "Package '{}' is a member and should be in the packages list",
                        package_id
                    )
                });
            let manifest = Manifest::new(&package.manifest_path)?;
            Ok((package_id.clone(), (package.clone(), manifest)))
        };

        let members = metadata
            .workspace_members
            .iter()
            .map(member_manifest)
            .collect::<Result<HashMap<_, _>>>()?;

        if !members.contains_key(root_package) {
            anyhow::bail!("The root package should be a workspace member")
        }

        Ok(Workspace {
            workspace_root: metadata.workspace_root.clone(),
            root_package: root_package.clone(),
            members,
        })
    }

    /// Amend the root package manifest using the supplied function.
    ///
    /// # Note
    ///
    /// The root package is the current workspace package being built, not to be confused with
    /// the workspace root (where the top level workspace Cargo.toml is defined).
    pub fn with_root_package_manifest<F>(&mut self, f: F) -> Result<&mut Self>
    where
        F: FnOnce(&mut Manifest) -> Result<()>,
    {
        let root_package_manifest = self
            .members
            .get_mut(&self.root_package)
            .map(|(_, m)| m)
            .expect("The root package should be a workspace member");
        f(root_package_manifest)?;
        Ok(self)
    }

    /// Amend the workspace manifest using the supplied function.
    pub fn with_workspace_manifest<F>(&mut self, f: F) -> Result<&mut Self>
    where
        F: FnOnce(&mut Manifest) -> Result<()>,
    {
        let workspace_root = self.workspace_root.clone();
        let workspace_manifest = self
            .members
            .iter_mut()
            .find_map(|(_, (_, manifest))| {
                if manifest.path().directory() == Some(&workspace_root) {
                    Some(manifest)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                anyhow::anyhow!("The workspace root package should be a workspace member")
            })?;
        f(workspace_manifest)?;
        Ok(self)
    }

    /// Generates a package to invoke for generating contract metadata
    pub(super) fn with_metadata_gen_package(&mut self) -> Result<&mut Self> {
        self.with_workspace_manifest(|manifest| {
            manifest.with_metadata_package()?;
            Ok(())
        })
    }

    /// Writes the amended manifests to the `target` directory, retaining the workspace directory
    /// structure, but only with the `Cargo.toml` files.
    ///
    /// Relative paths will be rewritten to absolute paths from the original workspace root, except
    /// intra-workspace relative dependency paths which will be preserved.
    ///
    /// Returns the paths of the new manifests.
    pub fn write<P: AsRef<Path>>(&mut self, target: P) -> Result<Vec<(PackageId, ManifestPath)>> {
        let exclude_member_package_names = self
            .members
            .iter()
            .map(|(_, (p, _))| p.name.clone())
            .collect::<Vec<_>>();
        let mut new_manifest_paths = Vec::new();
        for (package_id, (package, manifest)) in self.members.iter_mut() {
            // replace the original workspace root with the temporary directory
            let mut new_path: PathBuf = target.as_ref().into();
            new_path.push(package.manifest_path.strip_prefix(&self.workspace_root)?);
            let new_manifest = ManifestPath::new(new_path)?;

            manifest.rewrite_relative_paths(&exclude_member_package_names)?;
            manifest.write(&new_manifest)?;

            new_manifest_paths.push((package_id.clone(), new_manifest));
        }
        Ok(new_manifest_paths)
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
        log::debug!("Using temp workspace at '{}'", tmp_dir.path().display());
        let new_paths = self.write(&tmp_dir)?;
        let root_manifest_path = new_paths
            .iter()
            .find_map(|(pid, path)| {
                if *pid == self.root_package {
                    Some(path)
                } else {
                    None
                }
            })
            .expect("root package should be a member of the temp workspace");
        f(root_manifest_path)
    }
}

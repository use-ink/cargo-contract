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

use crate::{workspace::ManifestPath, Verbosity};
use anyhow::{Context, Result};
use cargo_metadata::{Metadata as CargoMetadata, MetadataCommand, PackageId, Package};
use rustc_version::Channel;
use std::{ffi::OsStr, path::{Path, PathBuf}, process::Command};

/// Relevant metadata obtained from Cargo.toml.
#[derive(Debug)]
pub struct CrateMetadata {
	pub manifest_path: ManifestPath,
	pub cargo_meta: cargo_metadata::Metadata,
	pub package_name: String,
	pub root_package: Package,
	pub original_wasm: PathBuf,
	pub dest_wasm: PathBuf,
}

impl CrateMetadata {
	/// Parses the contract manifest and returns relevant metadata.
	pub fn collect(manifest_path: &ManifestPath) -> Result<Self> {
		let (metadata, root_package_id) = get_cargo_metadata(manifest_path)?;

		// Find the root package by id in the list of packages. It is logical error if the root
		// package is not found in the list.
		let root_package = metadata
			.packages
			.iter()
			.find(|package| package.id == root_package_id)
			.expect("The package is not found in the `cargo metadata` output")
			.clone();

		// Normalize the package name.
		let package_name = root_package.name.replace("-", "_");

		// {target_dir}/wasm32-unknown-unknown/release/{package_name}.wasm
		let mut original_wasm = metadata.target_directory.clone();
		original_wasm.push("wasm32-unknown-unknown");
		original_wasm.push("release");
		original_wasm.push(package_name.clone());
		original_wasm.set_extension("wasm");

		// {target_dir}/{package_name}.wasm
		let mut dest_wasm = metadata.target_directory.clone();
		dest_wasm.push(package_name.clone());
		dest_wasm.set_extension("wasm");

		let crate_metadata = CrateMetadata {
			manifest_path: manifest_path.clone(),
			cargo_meta: metadata,
			root_package: root_package.clone(),
			package_name,
			original_wasm,
			dest_wasm,
		};
		Ok(crate_metadata)
	}
}

/// Get the result of `cargo metadata`, together with the root package id.
fn get_cargo_metadata(manifest_path: &ManifestPath) -> Result<(CargoMetadata, PackageId)> {
	let mut cmd = MetadataCommand::new();
	let metadata = cmd
		.manifest_path(manifest_path)
		.exec()
		.context("Error invoking `cargo metadata`")?;
	let root_package_id = metadata
		.resolve
		.as_ref()
		.and_then(|resolve| resolve.root.as_ref())
		.context("Cannot infer the root project id")?
		.clone();
	Ok((metadata, root_package_id))
}

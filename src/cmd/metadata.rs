// Copyright 2018-2019 Parity Technologies (UK) Ltd.
// This file is part of ink!.
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

use crate::{
    util,
    workspace::{ManifestPath, Workspace},
};
use anyhow::Result;

/// Executes build of the smart-contract which produces a wasm binary that is ready for deploying.
///
/// It does so by invoking build by cargo and then post processing the final binary.
pub(crate) fn execute_generate_metadata(manifest_path: ManifestPath) -> Result<String> {
    util::assert_channel()?;
    println!("  Generating metadata");

    let (metadata, root_package_id) = crate::util::get_cargo_metadata(&manifest_path)?;

    let mut workspace = Workspace::new(&metadata, &root_package_id)?;
    workspace
        .root_package_manifest_mut()
        .with_added_crate_type("rlib")?;
    workspace.using_temp(|root_manifest_path| {
        let target_dir = format!(
            "--target-dir={}",
            metadata.target_directory.to_string_lossy()
        );
        util::invoke_cargo(
            "run",
            &[
                "--package",
                "abi-gen",
                &root_manifest_path.cargo_arg(),
                &target_dir,
                "--release",
                // "--no-default-features", // Breaks builds for MacOS (linker errors), we should investigate this issue asap!
            ],
        )
    })?;

    let mut out_path = metadata.target_directory;
    out_path.push("metadata.json");

    Ok(format!(
        "Your metadata file is ready.\nYou can find it here:\n{}",
        out_path.display()
    ))
}

#[cfg(feature = "test-ci-only")]
#[cfg(test)]
mod tests {
    use crate::{
        cmd::{execute_generate_metadata, execute_new},
        util::tests::with_tmp_dir,
        workspace::ManifestPath,
    };

    #[test]
    fn generate_metadata() {
        with_tmp_dir(|path| {
            execute_new("new_project", Some(path)).expect("new project creation failed");
            let working_dir = path.join("new_project");
            let manifest_path = ManifestPath::new(working_dir.join("Cargo.toml")).unwrap();
            execute_generate_metadata(manifest_path).expect("generate metadata failed");

            let mut abi_file = working_dir;
            abi_file.push("target");
            abi_file.push("metadata.json");
            assert!(abi_file.exists())
        });
    }
}

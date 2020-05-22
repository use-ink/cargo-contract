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
    Verbosity,
    UnstableFlags,
};
use anyhow::Result;

const METADATA_FILE: &str = "metadata.json";

/// Executes build of the smart-contract which produces a wasm binary that is ready for deploying.
///
/// It does so by invoking build by cargo and then post processing the final binary.
pub(crate) fn execute_generate_metadata(
    manifest_path: ManifestPath,
    verbosity: Option<Verbosity>,
    unstable_options: UnstableFlags,
) -> Result<String> {
    util::assert_channel()?;
    println!("  Generating metadata");

    let (metadata, root_package_id) = crate::util::get_cargo_metadata(&manifest_path)?;

    let out_path = metadata.target_directory.join(METADATA_FILE);
    let out_path_display = format!("{}", out_path.display());

    let target_dir = metadata.target_directory.clone();

    let generate_metadata = move |manifest_path: &ManifestPath| -> Result<()> {
        let target_dir_arg = format!("--target-dir={}", target_dir.to_string_lossy());
        util::invoke_cargo(
            "run",
            &[
                "--package",
                "abi-gen",
                &manifest_path.cargo_arg(),
                &target_dir_arg,
                "--release",
                // "--no-default-features", // Breaks builds for MacOS (linker errors), we should investigate this issue asap!
            ],
            manifest_path.directory(),
            verbosity,
        )
    };

    if unstable_options.original_manifest {
        generate_metadata(&manifest_path)?;
    } else {
        Workspace::new(&metadata, &root_package_id)?
            .with_root_package_manifest(|manifest| {
                manifest
                    .with_added_crate_type("rlib")?
                    .with_profile_release_lto(false)?;
                Ok(())
            })?
            .using_temp(generate_metadata)?;
    }

    Ok(format!(
        "Your metadata file is ready.\nYou can find it here:\n{}",
        out_path_display
    ))
}

#[cfg(feature = "test-ci-only")]
#[cfg(test)]
mod tests {
    use crate::{
        cmd::{execute_generate_metadata, execute_new},
        util::tests::with_tmp_dir,
        workspace::ManifestPath,
        UnstableFlags,
    };

    #[test]
    fn generate_metadata() {
        env_logger::try_init().ok();
        with_tmp_dir(|path| {
            execute_new("new_project", Some(path)).expect("new project creation failed");
            let working_dir = path.join("new_project");
            let manifest_path = ManifestPath::new(working_dir.join("Cargo.toml")).unwrap();
            let message =
                execute_generate_metadata(manifest_path, None, UnstableFlags::default()).expect("generate metadata failed");
            println!("{}", message);

            let mut abi_file = working_dir;
            abi_file.push("target");
            abi_file.push("metadata.json");
            assert!(
                abi_file.exists(),
                format!("Missing metadata file '{}'", abi_file.display())
            )
        });
    }
}

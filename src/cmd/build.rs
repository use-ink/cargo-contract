// Copyright 2018-2020 Parity Technologies (UK) Ltd.
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
    fs::{metadata, File},
    io::{Read, Write},
    path::PathBuf,
};

use crate::{
    crate_metadata::CrateMetadata,
    util,
    workspace::{ManifestPath, Profile, Workspace},
    UnstableFlags, Verbosity,
};
use anyhow::{Context, Result};
use colored::Colorize;
use parity_wasm::elements::{External, MemoryType, Module, Section};

/// This is the maximum number of pages available for a contract to allocate.
const MAX_MEMORY_PAGES: u32 = 16;

/// Result of the metadata generation process.
pub struct BuildResult {
    /// Path to the resulting metadata file.
    pub dest_metadata: Option<PathBuf>,
    /// Path to the resulting Wasm file.
    pub dest_wasm: Option<PathBuf>,
    /// Path to the bundled file.
    pub dest_bundle: Option<PathBuf>,
}

/// Builds the project in the specified directory, defaults to the current directory.
///
/// Uses [`cargo-xbuild`](https://github.com/rust-osdev/cargo-xbuild) for maximum optimization of
/// the resulting Wasm binary.
///
/// # Cargo.toml optimizations
///
/// The original Cargo.toml will be amended to remove the `rlib` crate type in order to minimize
/// the final Wasm binary size.
///
/// Preferred default `[profile.release]` settings will be added if they are missing, existing
/// user-defined settings will be preserved.
///
/// To disable this and use the original `Cargo.toml` as is then pass the `-Z original_manifest` flag.
fn build_cargo_project(
    crate_metadata: &CrateMetadata,
    verbosity: Option<Verbosity>,
    unstable_options: UnstableFlags,
) -> Result<()> {
    util::assert_channel()?;

    // set RUSTFLAGS, read from environment var by cargo-xbuild
    std::env::set_var(
        "RUSTFLAGS",
        "-C link-arg=-z -C link-arg=stack-size=65536 -C link-arg=--import-memory",
    );

    let verbosity = verbosity.map(|v| match v {
        Verbosity::Verbose => xargo_lib::Verbosity::Verbose,
        Verbosity::Quiet => xargo_lib::Verbosity::Quiet,
    });

    let xbuild = |manifest_path: &ManifestPath| {
        let manifest_path = Some(manifest_path);
        let target = Some("wasm32-unknown-unknown");
        let target_dir = &crate_metadata.cargo_meta.target_directory;
        let other_args = [
            "--no-default-features",
            "--release",
            &format!("--target-dir={}", target_dir.to_string_lossy()),
        ];
        let args = xargo_lib::Args::new(target, manifest_path, verbosity, &other_args)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context("Creating xargo args")?;

        let config = xargo_lib::Config {
            sysroot_path: target_dir.join("sysroot"),
            memcpy: false,
            panic_immediate_abort: true,
        };

        let exit_status = xargo_lib::build(args, "build", Some(config))
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context("Building with xbuild")?;
        if !exit_status.success() {
            anyhow::bail!("xbuild failed with status {}", exit_status)
        }
        Ok(())
    };

    if unstable_options.original_manifest {
        println!(
            "{} {}",
            "warning:".yellow().bold(),
            "with 'original-manifest' enabled, the contract binary may not be of optimal size."
                .bold()
        );
        xbuild(&crate_metadata.manifest_path)?;
    } else {
        Workspace::new(&crate_metadata.cargo_meta, &crate_metadata.root_package.id)?
            .with_root_package_manifest(|manifest| {
                manifest
                    .with_removed_crate_type("rlib")?
                    .with_profile_release_defaults(Profile::default_contract_release())?;
                Ok(())
            })?
            .using_temp(xbuild)?;
    }

    // clear RUSTFLAGS
    std::env::remove_var("RUSTFLAGS");

    Ok(())
}

/// Ensures the wasm memory import of a given module has the maximum number of pages.
///
/// Iterates over the import section, finds the memory import entry if any and adjusts the maximum
/// limit.
fn ensure_maximum_memory_pages(module: &mut Module, maximum_allowed_pages: u32) -> Result<()> {
    let mem_ty = module
        .import_section_mut()
        .and_then(|section| {
            section
                .entries_mut()
                .iter_mut()
                .find_map(|entry| match entry.external_mut() {
                    External::Memory(ref mut mem_ty) => Some(mem_ty),
                    _ => None,
                })
        })
        .context("Memory import is not found. Is --import-memory specified in the linker args")?;

    if let Some(requested_maximum) = mem_ty.limits().maximum() {
        // The module already has maximum, check if it is within the limit bail out.
        if requested_maximum > maximum_allowed_pages {
            anyhow::bail!(
                "The wasm module requires {} pages. The maximum allowed number of pages is {}",
                requested_maximum,
                maximum_allowed_pages,
            );
        }
    } else {
        let initial = mem_ty.limits().initial();
        *mem_ty = MemoryType::new(initial, Some(MAX_MEMORY_PAGES));
    }

    Ok(())
}

/// Strips all custom sections.
///
/// Presently all custom sections are not required so they can be stripped safely.
fn strip_custom_sections(module: &mut Module) {
    module.sections_mut().retain(|section| match section {
        Section::Custom(_) => false,
        Section::Name(_) => false,
        Section::Reloc(_) => false,
        _ => true,
    });
}

/// Performs required post-processing steps on the wasm artifact.
fn post_process_wasm(crate_metadata: &CrateMetadata) -> Result<()> {
    // Deserialize wasm module from a file.
    let mut module =
        parity_wasm::deserialize_file(&crate_metadata.original_wasm).context(format!(
            "Loading original wasm file '{}'",
            crate_metadata.original_wasm.display()
        ))?;

    // Perform optimization.
    //
    // In practice only tree-shaking is performed, i.e transitively removing all symbols that are
    // NOT used by the specified entrypoints.
    if pwasm_utils::optimize(&mut module, ["call", "deploy"].to_vec()).is_err() {
        anyhow::bail!("Optimizer failed");
    }
    ensure_maximum_memory_pages(&mut module, MAX_MEMORY_PAGES)?;
    strip_custom_sections(&mut module);

    parity_wasm::serialize_to_file(&crate_metadata.dest_wasm, module)?;
    Ok(())
}

/// Attempts to perform optional wasm optimization using `binaryen`.
///
/// The intention is to reduce the size of bloated wasm binaries as a result of missing
/// optimizations (or bugs?) between Rust and Wasm.
fn optimize_wasm(crate_metadata: &CrateMetadata) -> Result<()> {
    let mut optimized = crate_metadata.dest_wasm.clone();
    optimized.set_file_name(format!("{}-opt.wasm", crate_metadata.package_name));

    let codegen_config = binaryen::CodegenConfig {
        // execute -O3 optimization passes (spends potentially a lot of time optimizing)
        optimization_level: 3,
        // the default
        shrink_level: 1,
        // the default
        debug_info: false,
    };

    let mut dest_wasm_file = File::open(crate_metadata.dest_wasm.as_os_str())?;
    let mut dest_wasm_file_content = Vec::new();
    dest_wasm_file.read_to_end(&mut dest_wasm_file_content)?;

    let mut module = binaryen::Module::read(&dest_wasm_file_content)
        .map_err(|_| anyhow::anyhow!("binaryen failed to read file content"))?;
    module.optimize(&codegen_config);
    let optimized_wasm = module.write();

    let mut optimized_wasm_file = File::create(optimized.as_os_str())?;
    optimized_wasm_file.write_all(&optimized_wasm)?;

    let original_size = metadata(&crate_metadata.dest_wasm)?.len() as f64 / 1000.0;
    let optimized_size = metadata(&optimized)?.len() as f64 / 1000.0;
    println!(
        " Original wasm size: {:.1}K, Optimized: {:.1}K",
        original_size, optimized_size
    );

    // overwrite existing destination wasm file with the optimised version
    std::fs::rename(&optimized, &crate_metadata.dest_wasm)?;
    Ok(())
}

/// Executes build of the smart-contract which produces a wasm binary that is ready for deploying.
///
/// It does so by invoking `cargo build` and then post processing the final binary.
///
/// # Note
///
/// Collects the contract crate's metadata using the supplied manifest (`Cargo.toml`) path. Use
/// [`execute_build_with_metadata`] if an instance is already available.
pub(crate) fn execute(
    manifest_path: &ManifestPath,
    verbosity: Option<Verbosity>,
    optimize_contract: bool,
    skip_bundle: bool,
    skip_metadata: bool,
    unstable_options: UnstableFlags,
) -> Result<BuildResult> {
    let crate_metadata = CrateMetadata::collect(manifest_path)?;
    if skip_metadata {
        let total_steps = 3;
        let dest_wasm = execute_with_metadata(
            &crate_metadata,
            verbosity,
            optimize_contract,
            unstable_options,
            total_steps,
        )?;
        let res = BuildResult {
            dest_wasm,
            dest_metadata: None,
            dest_bundle: None,
        };
        return Ok(res);
    }

    let total_steps = if skip_bundle { 4 } else { 5 };
    let metadata_result = super::metadata::execute(
        &manifest_path,
        verbosity,
        !skip_bundle,
        optimize_contract,
        unstable_options,
        total_steps,
    )?;
    let res = BuildResult {
        dest_wasm: Some(metadata_result.wasm_file),
        dest_metadata: Some(metadata_result.metadata_file),
        dest_bundle: metadata_result.bundle_file,
    };
    Ok(res)
}

/// Executes build of the smart-contract which produces a wasm binary that is ready for deploying.
///
/// It does so by invoking `cargo build` and then post processing the final binary.
///
/// # Note
///
/// Uses the supplied `CrateMetadata`. If an instance is not available use [`execute_build`]
pub(crate) fn execute_with_metadata(
    crate_metadata: &CrateMetadata,
    verbosity: Option<Verbosity>,
    optimize_contract: bool,
    unstable_options: UnstableFlags,
    total_steps: usize,
) -> Result<Option<PathBuf>> {
    println!(
        " {} {}",
        format!("[1/{}]", total_steps).bold(),
        "Building cargo project".bright_green().bold()
    );
    build_cargo_project(&crate_metadata, verbosity, unstable_options)?;
    println!(
        " {} {}",
        format!("[2/{}]", total_steps).bold(),
        "Post processing wasm file".bright_green().bold()
    );
    post_process_wasm(&crate_metadata)?;
    if !optimize_contract {
        return Ok(None);
    }
    println!(
        " {} {}",
        format!("[3/{}]", total_steps).bold(),
        "Optimizing wasm file".bright_green().bold()
    );
    optimize_wasm(&crate_metadata)?;
    Ok(Some(crate_metadata.dest_wasm.clone()))
}

#[cfg(feature = "test-ci-only")]
#[cfg(test)]
mod tests {
    use crate::{cmd, util::tests::with_tmp_dir, ManifestPath, UnstableFlags};

    #[test]
    fn build_template() {
        with_tmp_dir(|path| {
            cmd::new::execute("new_project", Some(path)).expect("new project creation failed");
            let manifest_path =
                ManifestPath::new(&path.join("new_project").join("Cargo.toml")).unwrap();
            super::execute(
                &manifest_path,
                None,
                true,
                false,
                false,
                UnstableFlags::default(),
            )
            .expect("build failed");
            Ok(())
        })
    }
}

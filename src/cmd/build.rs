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

use std::{
    fs::metadata,
    io::{self, Write},
    path::PathBuf,
    process::Command,
};

use crate::{manifest::CargoToml, util};
use anyhow::{Context, Result};
use cargo_metadata::Package;
use colored::Colorize;
use parity_wasm::elements::{External, MemoryType, Module, Section};
use serde_json::Value;

/// This is the maximum number of pages available for a contract to allocate.
const MAX_MEMORY_PAGES: u32 = 16;

/// Relevant metadata obtained from Cargo.toml.
#[derive(Debug)]
pub struct CrateMetadata {
    working_dir: Option<PathBuf>,
    workspace_root: PathBuf,
    package_name: String,
    root_package: Package,
    original_wasm: PathBuf,
    pub dest_wasm: PathBuf,
}

/// Parses the contract manifest and returns relevant metadata.
pub fn collect_crate_metadata(working_dir: Option<&PathBuf>) -> Result<CrateMetadata> {
    let metadata = crate::util::get_cargo_metadata(working_dir)?;

    let root_package_id = metadata
        .resolve
        .and_then(|resolve| resolve.root)
        .context("Cannot infer the root project id")?;

    // Find the root package by id in the list of packages. It is logical error if the root
    // package is not found in the list.
    let root_package = metadata
        .packages
        .iter()
        .find(|package| package.id == root_package_id)
        .expect("The package is not found in the `cargo metadata` output");

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
        working_dir: working_dir.cloned(),
        workspace_root: metadata.workspace_root.clone(),
        root_package: root_package.clone(),
        package_name,
        original_wasm,
        dest_wasm,
    };
    log::debug!("{:#?}", crate_metadata);
    Ok(crate_metadata)
}

/// Builds the project in the specified directory, defaults to the current directory.
///
/// Uses [`cargo-xbuild`](https://github.com/rust-osdev/cargo-xbuild) for maximum optimization of
/// the resulting Wasm binary.
fn build_cargo_project(crate_metadata: &CrateMetadata) -> Result<()> {
    util::assert_channel()?;

    let target = "wasm32-unknown-unknown";
    let build_args = [
        "--no-default-features",
        "--release",
        &format!("--target={}", target),
    ];
    let manifest = CargoToml::from_working_dir(crate_metadata.working_dir.as_ref())?;

    // check `cargo-xbuild` config section exists and has `panic_immediate_abort` enabled
    let xbuild_metadata = crate_metadata.root_package.metadata.get("cargo-xbuild");
    if let Some(xbuild_metadata) = xbuild_metadata {
        let panic_immediate_abort_enabled = xbuild_metadata
            .get("panic_immediate_abort")
            .map_or(false, |v| *v == Value::Bool(true));
        if !panic_immediate_abort_enabled {
            anyhow::bail!(
                "For optimal binary size please set `panic_immediate_abort = true` in the \
                `[package.metadata.cargo-xbuild]` section of `Cargo.toml`. \
                See https://github.com/paritytech/cargo-contract#contract-build-config"
            )
        }
    } else {
        anyhow::bail!(
            "For optimal binary size please add a `[package.metadata.cargo-xbuild]` section to \
            `Cargo.toml` with `panic_immediate_abort = true` \
            See https://github.com/paritytech/cargo-contract#contract-build-config"
        )
    }

    // temporarily remove the 'rlib' crate-type to build wasm blob for optimal size
    manifest.with_removed_crate_type("rlib", || {
        let manifest_path = Some(manifest.manifest_path());
        let exit_status = xargo_lib::build(
            xargo_lib::Args::new(&build_args, Some(target), manifest_path),
            "build",
        )
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("Building with xbuild")?;
        log::debug!("xargo exit status: {:?}", exit_status);
        Ok(())
    })
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

/// Attempts to perform optional wasm optimization using `wasm-opt`.
///
/// The intention is to reduce the size of bloated wasm binaries as a result of missing
/// optimizations (or bugs?) between Rust and Wasm.
///
/// This step depends on the `wasm-opt` tool being installed. If it is not the build will still
/// succeed, and the user will be encouraged to install it for further optimizations.
fn optimize_wasm(crate_metadata: &CrateMetadata) -> Result<()> {
    // check `wasm-opt` installed
    if which::which("wasm-opt").is_err() {
        println!(
            "{}",
            "wasm-opt is not installed. Install this tool on your system in order to \n\
             reduce the size of your contract's Wasm binary. \n\
             See https://github.com/WebAssembly/binaryen#tools"
                .bright_yellow()
        );
        return Ok(());
    }

    let mut optimized = crate_metadata.dest_wasm.clone();
    optimized.set_file_name(format!("{}-opt.wasm", crate_metadata.package_name));

    let output = Command::new("wasm-opt")
        .arg(crate_metadata.dest_wasm.as_os_str())
        .arg("-O3") // execute -O3 optimization passes (spends potentially a lot of time optimizing)
        .arg("-o")
        .arg(optimized.as_os_str())
        .output()?;

    if !output.status.success() {
        // Dump the output streams produced by wasm-opt into the stdout/stderr.
        io::stdout().write_all(&output.stdout)?;
        io::stderr().write_all(&output.stderr)?;
        anyhow::bail!("wasm-opt optimization failed");
    }

    let original_size = metadata(&crate_metadata.dest_wasm)?.len() / 1000;
    let optimized_size = metadata(&optimized)?.len() / 1000;
    println!(
        " Original wasm size: {}K, Optimized: {}K",
        original_size, optimized_size
    );

    // overwrite existing destination wasm file with the optimised version
    std::fs::rename(&optimized, &crate_metadata.dest_wasm)?;
    Ok(())
}

/// Executes build of the smart-contract which produces a wasm binary that is ready for deploying.
///
/// It does so by invoking build by cargo and then post processing the final binary.
pub(crate) fn execute_build(working_dir: Option<&PathBuf>) -> Result<String> {
    println!(
        " {} {}",
        "[1/4]".bold(),
        "Collecting crate metadata".bright_green().bold()
    );
    let crate_metadata = collect_crate_metadata(working_dir)?;
    println!(
        " {} {}",
        "[2/4]".bold(),
        "Building cargo project".bright_green().bold()
    );
    build_cargo_project(&crate_metadata)?;
    println!(
        " {} {}",
        "[3/4]".bold(),
        "Post processing wasm file".bright_green().bold()
    );
    post_process_wasm(&crate_metadata)?;
    println!(
        " {} {}",
        "[4/4]".bold(),
        "Optimizing wasm file".bright_green().bold()
    );
    optimize_wasm(&crate_metadata)?;

    Ok(format!(
        "\nYour contract is ready. You can find it here:\n{}",
        crate_metadata.dest_wasm.display().to_string().bold()
    ))
}

#[cfg(feature = "test-ci-only")]
#[cfg(test)]
mod tests {
    use crate::cmd::{execute_new, util::tests::with_tmp_dir};

    #[test]
    fn build_template() {
        with_tmp_dir(|path| {
            execute_new("new_project", Some(path)).expect("new project creation failed");
            super::execute_build(Some(&path.join("new_project"))).expect("build failed");
        });
    }
}

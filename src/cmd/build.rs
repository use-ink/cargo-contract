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
    fs::{self, metadata},
    io::{self, Write},
    path::PathBuf,
    process::Command,
};

use crate::manifest::CargoToml;
use anyhow::{Context, Result};
use colored::Colorize;
use parity_wasm::elements::{External, MemoryType, Module, Section};

/// This is the maximum number of pages available for a contract to allocate.
const MAX_MEMORY_PAGES: u32 = 16;

/// Relevant metadata obtained from Cargo.toml.
pub struct CrateMetadata {
    workspace_root: PathBuf,
    manifest_path: PathBuf,
    package_name: String,
    original_wasm: PathBuf,
    pub dest_wasm: PathBuf,
}

/// Parses the contract manifest and returns relevant metadata.
pub fn collect_crate_metadata(working_dir: Option<&PathBuf>) -> Result<CrateMetadata> {
    let metadata = super::get_cargo_metadata(working_dir)?;

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

    Ok(CrateMetadata {
        workspace_root: metadata.workspace_root.clone(),
        manifest_path: metadata.workspace_root.join("Cargo.toml"),
        package_name,
        original_wasm,
        dest_wasm,
    })
}

/// Generate a Xargo.config file for optimized wasm build.
/// If the file does not exist it will be generated and cleaned up afterwards.
/// If the file does exist and is of the required configuration then it will be left as is.
///
/// # Errors
///
/// - If there is an existing Xargo.config without the required configuration
fn with_xargo_config<F>(crate_metadata: &CrateMetadata, f: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    let xargo_config_path = crate_metadata.workspace_root.join("Xargo.toml");

    let xargo_config = r#"
[target.wasm32-unknown-unknown.dependencies]
core = {default-features=false, features=["panic_immediate_abort"]}
std = {default-features=false, features=["panic_immediate_abort"]}
alloc = {}
"#;

    // If there is an existing Xargo.toml which is different to the config above, instead of
    // overwriting it fail the build instead.
    let open_result = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(xargo_config_path.clone());

    match open_result {
        Ok(mut outfile) => {
            // No existing Xargo.toml
            outfile.write_all(xargo_config.as_bytes())?;
            let res = f();
            // clean up the auto generated Xargo.config: the user should not be editing this file
            let _ = fs::remove_file(xargo_config_path);
            res
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                let existing = fs::read_to_string(xargo_config_path).expect("File exists");
                if existing != xargo_config {
                    anyhow::bail!(
                        "A Xargo.config already exists which is different to the recommended \
                         configuration. Please remove/rename it and run again",
                    )
                }
                // Xargo.config file already exists with correct content so just use it
                Ok(())
            } else {
                anyhow::bail!(e)
            }
        }
    }
}

/// Builds the project in the specified directory, defaults to the current directory.
///
/// Attempts to build using [`xargo`](https://github.com/japaric/xargo) for maximum optimization of
/// the resulting Wasm binary.
///
/// If `xargo` is not installed then the user will be warned and it will fall back to `cargo`.
fn build_cargo_project(crate_metadata: &CrateMetadata) -> Result<()> {
    let build_args = [
        "--no-default-features",
        "--release",
        "--target=wasm32-unknown-unknown",
        "--verbose",
    ];
    let manifest = CargoToml::new(&crate_metadata.manifest_path)?;
    let working_dir = Some(&crate_metadata.workspace_root);

    // temporarily remove the 'rlib' crate-type to build wasm blob for optimal size
    manifest.with_removed_crate_type("rlib", || {
        // prefer building with xargo for optimal size, but fall back to cargo
        if which::which("xargo").is_err() {
            println!("TODO: tell the user nicely to install xargo");
            super::rustup_run("cargo", "build", &build_args, working_dir)
        } else {
            with_xargo_config(crate_metadata, || {
                super::rustup_run("xargo", "build", &build_args, working_dir)
            })
        }
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
    let mut module = parity_wasm::deserialize_file(&crate_metadata.original_wasm)?;

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
    use crate::cmd::{execute_new, tests::with_tmp_dir};

    #[test]
    fn build_template() {
        with_tmp_dir(|path| {
            execute_new("new_project", Some(path)).expect("new project creation failed");
            super::execute_build(Some(&path.join("new_project"))).expect("build failed");
        });
    }
}

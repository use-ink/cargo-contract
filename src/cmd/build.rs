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
    convert::TryFrom,
    fs::{metadata, File},
    io::{Read, Write},
    path::PathBuf,
};

use crate::{
    crate_metadata::CrateMetadata,
    util,
    workspace::{ManifestPath, Profile, Workspace},
    BuildArtifacts, BuildResult, UnstableFlags, UnstableOptions, VerbosityFlags,
};
use crate::{OptimizationResult, Verbosity};
use anyhow::{Context, Result};
use colored::Colorize;
use parity_wasm::elements::{External, MemoryType, Module, Section};
use structopt::StructOpt;

/// This is the maximum number of pages available for a contract to allocate.
const MAX_MEMORY_PAGES: u32 = 16;

/// Executes build of the smart-contract which produces a wasm binary that is ready for deploying.
///
/// It does so by invoking `cargo build` and then post processing the final binary.
#[derive(Debug, StructOpt)]
#[structopt(name = "build")]
pub struct BuildCommand {
    /// Path to the Cargo.toml of the contract to build
    #[structopt(long, parse(from_os_str))]
    manifest_path: Option<PathBuf>,
    /// Which build artifacts to generate.
    ///
    /// - `all`: Generate the Wasm, the metadata and a bundled `<name>.contract` file.
    ///
    /// - `code-only`: Only the Wasm is created, generation of metadata and a bundled
    ///   `<name>.contract` file is skipped.
    #[structopt(
        long = "generate",
        default_value = "all",
        value_name = "all | code-only",
        verbatim_doc_comment
    )]
    build_artifact: BuildArtifacts,
    #[structopt(flatten)]
    verbosity: VerbosityFlags,
    #[structopt(flatten)]
    unstable_options: UnstableOptions,
    /// Emits debug info into wasm file
    #[structopt(long, short)]
    debug: bool,
}

impl BuildCommand {
    pub fn exec(&self) -> Result<BuildResult> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let unstable_flags: UnstableFlags =
            TryFrom::<&UnstableOptions>::try_from(&self.unstable_options)?;
        let verbosity: Option<Verbosity> = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;
        execute(
            &manifest_path,
            verbosity,
            true,
            self.build_artifact,
            unstable_flags,
            self.debug,
        )
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "check")]
pub struct CheckCommand {
    /// Path to the Cargo.toml of the contract to build
    #[structopt(long, parse(from_os_str))]
    manifest_path: Option<PathBuf>,
    #[structopt(flatten)]
    verbosity: VerbosityFlags,
    #[structopt(flatten)]
    unstable_options: UnstableOptions,
}

impl CheckCommand {
    pub fn exec(&self) -> Result<BuildResult> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let unstable_flags: UnstableFlags =
            TryFrom::<&UnstableOptions>::try_from(&self.unstable_options)?;
        let verbosity: Option<Verbosity> = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;
        execute(
            &manifest_path,
            verbosity,
            false,
            BuildArtifacts::CheckOnly,
            unstable_flags,
            false,
        )
    }
}

/// Builds the project in the specified directory, defaults to the current directory.
///
/// Uses the unstable cargo feature [`build-std`](https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#build-std)
/// to build the standard library with [`panic_immediate_abort`](https://github.com/johnthagen/min-sized-rust#remove-panic-string-formatting-with-panic_immediate_abort)
/// which reduces the size of the Wasm binary by not including panic strings and formatting code.
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
    unstable_flags: UnstableFlags,
    debug: bool,
) -> Result<()> {
    util::assert_channel()?;

    // set linker args via RUSTFLAGS.
    // Currently will override user defined RUSTFLAGS from .cargo/config. See https://github.com/paritytech/cargo-contract/issues/98.
    let mut flags =
        "-C link-arg=-z -C link-arg=stack-size=65536 -C link-arg=--import-memory".to_string();
    if debug {
        flags.push_str(" -C opt-level=1");
    }
    std::env::set_var("RUSTFLAGS", flags);

    let cargo_build = |manifest_path: &ManifestPath| {
        let target_dir = &crate_metadata.target_directory;
        util::invoke_cargo(
            "build",
            &[
                "--target=wasm32-unknown-unknown",
                "-Zbuild-std",
                "-Zbuild-std-features=panic_immediate_abort",
                "--no-default-features",
                "--release",
                &format!("--target-dir={}", target_dir.to_string_lossy()),
            ],
            manifest_path.directory(),
            verbosity,
        )?;
        Ok(())
    };

    if unstable_flags.original_manifest {
        println!(
            "{} {}",
            "warning:".yellow().bold(),
            "with 'original-manifest' enabled, the contract binary may not be of optimal size."
                .bold()
        );
        cargo_build(&crate_metadata.manifest_path)?;
    } else {
        Workspace::new(&crate_metadata.cargo_meta, &crate_metadata.root_package.id)?
            .with_root_package_manifest(|manifest| {
                manifest
                    .with_removed_crate_type("rlib")?
                    .with_profile_release_defaults(Profile::default_contract_release())?;
                Ok(())
            })?
            .using_temp(cargo_build)?;
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
fn strip_custom_sections(module: &mut Module, debug: bool) {
    module.sections_mut().retain(|section| match section {
        Section::Custom(_) => false,
        Section::Name(_) => debug,
        Section::Reloc(_) => false,
        _ => true,
    });
}

/// Performs required post-processing steps on the wasm artifact.
fn post_process_wasm(crate_metadata: &CrateMetadata, debug: bool) -> Result<()> {
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
    strip_custom_sections(&mut module, debug);

    parity_wasm::serialize_to_file(&crate_metadata.dest_wasm, module)?;
    Ok(())
}

/// Attempts to perform optional wasm optimization using `binaryen`.
///
/// The intention is to reduce the size of bloated wasm binaries as a result of missing
/// optimizations (or bugs?) between Rust and Wasm.
fn optimize_wasm(
    crate_metadata: &CrateMetadata,
    debug_info: bool,
) -> Result<(OptimizationResult, Option<PathBuf>)> {
    let mut optimized = crate_metadata.dest_wasm.clone();
    optimized.set_file_name(format!("{}-opt.wasm", crate_metadata.package_name));

    let codegen_config = binaryen::CodegenConfig {
        // execute -O3 optimization passes (spends potentially a lot of time optimizing)
        optimization_level: 3,
        // the default
        shrink_level: 1,
        // the default
        debug_info,
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

    // move debug source wasm file to `*.src.wasm`
    let mut maybe_debug_wasm = None;
    if debug_info {
        let debug_wasm = PathBuf::from(
            &crate_metadata
                .dest_wasm
                .to_string_lossy()
                .replace(".wasm", ".src.wasm"),
        );
        std::fs::rename(&crate_metadata.dest_wasm, &debug_wasm)?;
        maybe_debug_wasm = Some(debug_wasm);
    }

    // overwrite existing destination wasm file with the optimised version
    std::fs::rename(&optimized, &crate_metadata.dest_wasm)?;
    Ok((
        OptimizationResult {
            original_size,
            optimized_size,
        },
        maybe_debug_wasm,
    ))
}

/// Executes build of the smart-contract which produces a wasm binary that is ready for deploying.
///
/// It does so by invoking `cargo build` and then post processing the final binary.
fn execute(
    manifest_path: &ManifestPath,
    verbosity: Option<Verbosity>,
    optimize_contract: bool,
    build_artifact: BuildArtifacts,
    unstable_flags: UnstableFlags,
    debug: bool,
) -> Result<BuildResult> {
    let crate_metadata = CrateMetadata::collect(manifest_path)?;
    if build_artifact == BuildArtifacts::CodeOnly || build_artifact == BuildArtifacts::CheckOnly {
        let (maybe_dest_wasm, maybe_dest_debug_wasm, maybe_optimization_result) =
            execute_with_crate_metadata(
                &crate_metadata,
                verbosity,
                optimize_contract,
                build_artifact,
                unstable_flags,
                debug,
            )?;
        let res = BuildResult {
            dest_wasm: maybe_dest_wasm,
            maybe_dest_debug_wasm,
            dest_metadata: None,
            dest_bundle: None,
            target_directory: crate_metadata.target_directory,
            optimization_result: maybe_optimization_result,
            build_artifact,
        };
        return Ok(res);
    }

    let res = super::metadata::execute(
        &manifest_path,
        verbosity,
        build_artifact,
        unstable_flags,
        debug,
    )?;
    Ok(res)
}

/// Executes build of the smart-contract which produces a Wasm binary that is ready for deploying.
///
/// It does so by invoking `cargo build` and then post processing the final binary.
///
/// # Note
///
/// Uses the supplied `CrateMetadata`. If an instance is not available use [`execute_build`]
///
/// Returns a tuple of `(maybe_optimized_wasm_path, maybe_optimization_result)`.
pub(crate) fn execute_with_crate_metadata(
    crate_metadata: &CrateMetadata,
    verbosity: Option<Verbosity>,
    optimize_contract: bool,
    build_artifact: BuildArtifacts,
    unstable_flags: UnstableFlags,
    debug: bool,
) -> Result<(Option<PathBuf>, Option<PathBuf>, Option<OptimizationResult>)> {
    println!(
        " {} {}",
        format!("[1/{}]", build_artifact.steps()).bold(),
        "Building cargo project".bright_green().bold()
    );
    build_cargo_project(&crate_metadata, verbosity, unstable_flags, debug)?;
    println!(
        " {} {}",
        format!("[2/{}]", build_artifact.steps()).bold(),
        "Post processing wasm file".bright_green().bold()
    );
    post_process_wasm(&crate_metadata, debug)?;
    if !optimize_contract {
        return Ok((None, None, None));
    }
    println!(
        " {} {}",
        format!("[3/{}]", build_artifact.steps()).bold(),
        "Optimizing wasm file".bright_green().bold()
    );
    let (optimization_result, maybe_dest_debug_wasm) = optimize_wasm(&crate_metadata, debug)?;
    Ok((
        Some(crate_metadata.dest_wasm.clone()),
        maybe_dest_debug_wasm,
        Some(optimization_result),
    ))
}

#[cfg(feature = "test-ci-only")]
#[cfg(test)]
mod tests {
    use crate::{cmd, util::tests::with_tmp_dir, BuildArtifacts, ManifestPath, UnstableFlags};

    #[test]
    fn build_template() {
        with_tmp_dir(|path| {
            cmd::new::execute("new_project", Some(path)).expect("new project creation failed");
            let manifest_path =
                ManifestPath::new(&path.join("new_project").join("Cargo.toml")).unwrap();
            let res = super::execute(
                &manifest_path,
                None,
                true,
                BuildArtifacts::All,
                UnstableFlags::default(),
            )
            .expect("build failed");

            // we can't use `/target/ink` here, since this would match
            // for `/target` being the root path. but since `ends_with`
            // always matches whole path components we can be sure
            // the path can never be e.g. `foo_target/ink` -- the assert
            // would fail for that.
            assert!(res.target_directory.ends_with("target/ink"));
            Ok(())
        })
    }
}

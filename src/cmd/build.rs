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

use std::{convert::TryFrom, ffi::OsStr, fs::metadata, path::PathBuf};

#[cfg(feature = "binaryen-as-dependency")]
use std::{
    fs::File,
    io::{Read, Write},
};

#[cfg(not(feature = "binaryen-as-dependency"))]
use std::{process::Command, str};

use crate::{
    crate_metadata::CrateMetadata,
    maybe_println, util, validate_wasm,
    workspace::{ManifestPath, Profile, Workspace},
    BuildArtifacts, BuildResult, OptimizationResult, UnstableFlags, UnstableOptions, Verbosity,
    VerbosityFlags,
};
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
}

impl BuildCommand {
    pub fn exec(&self) -> Result<BuildResult> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let unstable_flags: UnstableFlags =
            TryFrom::<&UnstableOptions>::try_from(&self.unstable_options)?;
        let verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;
        execute(
            &manifest_path,
            verbosity,
            self.build_artifact,
            unstable_flags,
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
        let verbosity: Verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;
        execute(
            &manifest_path,
            verbosity,
            BuildArtifacts::CheckOnly,
            unstable_flags,
        )
    }
}

/// Executes the supplied cargo command on the project in the specified directory, defaults to the
/// current directory.
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
fn exec_cargo_for_wasm_target(
    crate_metadata: &CrateMetadata,
    command: &str,
    verbosity: Verbosity,
    unstable_flags: &UnstableFlags,
) -> Result<()> {
    util::assert_channel()?;

    // set linker args via RUSTFLAGS.
    // Currently will override user defined RUSTFLAGS from .cargo/config. See https://github.com/paritytech/cargo-contract/issues/98.
    std::env::set_var(
        "RUSTFLAGS",
        "-C link-arg=-z -C link-arg=stack-size=65536 -C link-arg=--import-memory",
    );

    let cargo_build = |manifest_path: &ManifestPath| {
        let target_dir = &crate_metadata.target_directory;
        let args = [
            "--target=wasm32-unknown-unknown",
            "-Zbuild-std",
            "-Zbuild-std-features=panic_immediate_abort",
            "--no-default-features",
            "--release",
            &format!("--target-dir={}", target_dir.to_string_lossy()),
        ];
        util::invoke_cargo(command, &args, manifest_path.directory(), verbosity)?;

        Ok(())
    };

    if unstable_flags.original_manifest {
        maybe_println!(
            verbosity,
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
fn strip_custom_sections(module: &mut Module) {
    module.sections_mut().retain(|section| {
        !matches!(
            section,
            Section::Custom(_) | Section::Name(_) | Section::Reloc(_)
        )
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

    validate_wasm::validate_import_section(&module)?;

    debug_assert!(
        !module.clone().to_bytes().unwrap().is_empty(),
        "resulting wasm size of post processing must be > 0"
    );

    parity_wasm::serialize_to_file(&crate_metadata.dest_wasm, module)?;
    Ok(())
}

/// Attempts to perform optional wasm optimization using `binaryen`.
///
/// The intention is to reduce the size of bloated wasm binaries as a result of missing
/// optimizations (or bugs?) between Rust and Wasm.
fn optimize_wasm(crate_metadata: &CrateMetadata) -> Result<OptimizationResult> {
    let mut dest_optimized = crate_metadata.dest_wasm.clone();
    dest_optimized.set_file_name(format!("{}-opt.wasm", crate_metadata.package_name));

    let _ = do_optimization(
        crate_metadata.dest_wasm.as_os_str(),
        &dest_optimized.as_os_str(),
        3,
    )?;

    let original_size = metadata(&crate_metadata.dest_wasm)?.len() as f64 / 1000.0;
    let optimized_size = metadata(&dest_optimized)?.len() as f64 / 1000.0;

    // overwrite existing destination wasm file with the optimised version
    std::fs::rename(&dest_optimized, &crate_metadata.dest_wasm)?;
    Ok(OptimizationResult {
        dest_wasm: crate_metadata.dest_wasm.clone(),
        original_size,
        optimized_size,
    })
}

/// Optimizes the Wasm supplied as `wasm` using the `binaryen-rs` dependency.
///
/// The supplied `optimization_level` denotes the number of optimization passes,
/// resulting in potentially a lot of time spent optimizing.
///
/// If successful, the optimized wasm is written to `dest_optimized`.
#[cfg(feature = "binaryen-as-dependency")]
fn do_optimization(
    dest_wasm: &OsStr,
    dest_optimized: &OsStr,
    optimization_level: u32,
) -> Result<()> {
    let mut dest_wasm_file = File::open(dest_wasm)?;
    let mut dest_wasm_file_content = Vec::new();
    dest_wasm_file.read_to_end(&mut dest_wasm_file_content)?;

    let codegen_config = binaryen::CodegenConfig {
        // number of optimization passes (spends potentially a lot of time optimizing)
        optimization_level,
        // the default
        shrink_level: 1,
        // the default
        debug_info: false,
    };
    let mut module = binaryen::Module::read(&dest_wasm_file_content)
        .map_err(|_| anyhow::anyhow!("binaryen failed to read file content"))?;
    module.optimize(&codegen_config);

    let mut optimized_wasm_file = File::create(dest_optimized)?;
    optimized_wasm_file.write_all(&module.write())?;

    Ok(())
}

/// Optimizes the Wasm supplied as `crate_metadata.dest_wasm` using
/// the `wasm-opt` binary.
///
/// The supplied `optimization_level` denotes the number of optimization passes,
/// resulting in potentially a lot of time spent optimizing.
///
/// If successful, the optimized wasm is written to `dest_optimized`.
#[cfg(not(feature = "binaryen-as-dependency"))]
fn do_optimization(
    dest_wasm: &OsStr,
    dest_optimized: &OsStr,
    optimization_level: u32,
) -> Result<()> {
    // check `wasm-opt` is installed
    if which::which("wasm-opt").is_err() {
        anyhow::bail!(
            "{}",
            "wasm-opt is not installed. Install this tool on your system in order to \n\
             reduce the size of your contract's Wasm binary. \n\
             See https://github.com/WebAssembly/binaryen#tools"
                .bright_yellow()
        );
    }

    let output = Command::new("wasm-opt")
        .arg(dest_wasm)
        .arg(format!("-O{}", optimization_level))
        .arg("-o")
        .arg(dest_optimized)
        // the memory in our module is imported, `wasm-opt` needs to be told that
        // the memory is initialized to zeros, otherwise it won't run the
        // memory-packing pre-pass.
        .arg("--zero-filled-memory")
        .output()?;

    if !output.status.success() {
        let err = str::from_utf8(&output.stderr)
            .expect("cannot convert stderr output of wasm-opt to string")
            .trim();
        anyhow::bail!(
            "The wasm-opt optimization failed.\n\
            A possible source of error could be that you have an outdated binaryen package installed.\n\n\
            The error which wasm-opt returned was: \n{}",
            err
        );
    }
    Ok(())
}

/// Executes build of the smart-contract which produces a wasm binary that is ready for deploying.
///
/// It does so by invoking `cargo build` and then post processing the final binary.
pub(crate) fn execute(
    manifest_path: &ManifestPath,
    verbosity: Verbosity,
    build_artifact: BuildArtifacts,
    unstable_flags: UnstableFlags,
) -> Result<BuildResult> {
    let crate_metadata = CrateMetadata::collect(manifest_path)?;

    let build = || -> Result<OptimizationResult> {
        maybe_println!(
            verbosity,
            " {} {}",
            format!("[1/{}]", build_artifact.steps()).bold(),
            "Building cargo project".bright_green().bold()
        );
        exec_cargo_for_wasm_target(&crate_metadata, "build", verbosity, &unstable_flags)?;

        maybe_println!(
            verbosity,
            " {} {}",
            format!("[2/{}]", build_artifact.steps()).bold(),
            "Post processing wasm file".bright_green().bold()
        );
        post_process_wasm(&crate_metadata)?;

        maybe_println!(
            verbosity,
            " {} {}",
            format!("[3/{}]", build_artifact.steps()).bold(),
            "Optimizing wasm file".bright_green().bold()
        );
        let optimization_result = optimize_wasm(&crate_metadata)?;

        Ok(optimization_result)
    };

    let (opt_result, metadata_result) = match build_artifact {
        BuildArtifacts::CheckOnly => {
            exec_cargo_for_wasm_target(&crate_metadata, "check", verbosity, &unstable_flags)?;
            (None, None)
        }
        BuildArtifacts::CodeOnly => {
            let optimization_result = build()?;
            (Some(optimization_result), None)
        }
        BuildArtifacts::All => {
            let optimization_result = build()?;

            let metadata_result = super::metadata::execute(
                &crate_metadata,
                optimization_result.dest_wasm.as_path(),
                verbosity,
                build_artifact.steps(),
                &unstable_flags,
            )?;
            (Some(optimization_result), Some(metadata_result))
        }
    };
    let dest_wasm = opt_result.as_ref().map(|r| r.dest_wasm.clone());
    Ok(BuildResult {
        dest_wasm,
        metadata_result,
        target_directory: crate_metadata.target_directory,
        optimization_result: opt_result,
        build_artifact,
        verbosity,
    })
}

#[cfg(feature = "test-ci-only")]
#[cfg(test)]
mod tests_ci_only {
    use crate::{
        cmd, util::tests::with_tmp_dir, BuildArtifacts, ManifestPath, UnstableFlags, Verbosity,
    };

    #[test]
    fn build_code_only() {
        with_tmp_dir(|path| {
            cmd::new::execute("new_project", Some(path)).expect("new project creation failed");
            let manifest_path =
                ManifestPath::new(&path.join("new_project").join("Cargo.toml")).unwrap();
            let res = super::execute(
                &manifest_path,
                Verbosity::Default,
                BuildArtifacts::CodeOnly,
                UnstableFlags::default(),
            )
            .expect("build failed");

            // our ci has set `CARGO_TARGET_DIR` to cache artifacts.
            // this dir does not include `/target/` as a path, hence
            // we can't match for e.g. `foo_project/target/ink`.
            //
            // we also can't match for `/ink` here, since this would match
            // for `/ink` being the root path.
            assert!(res.target_directory.ends_with("ink"));

            assert!(
                res.metadata_result.is_none(),
                "CodeOnly should not generate the metadata"
            );

            let optimized_size = res.optimization_result.unwrap().optimized_size;
            assert!(optimized_size > 0.0);

            // our optimized contract template should always be below 3k.
            assert!(optimized_size < 3.0);

            Ok(())
        })
    }

    #[test]
    fn check_must_not_output_contract_artifacts_in_project_dir() {
        with_tmp_dir(|path| {
            // given
            cmd::new::execute("new_project", Some(path)).expect("new project creation failed");
            let project_dir = path.join("new_project");
            let manifest_path = ManifestPath::new(&project_dir.join("Cargo.toml")).unwrap();

            // when
            super::execute(
                &manifest_path,
                Verbosity::Default,
                BuildArtifacts::CheckOnly,
                UnstableFlags::default(),
            )
            .expect("build failed");

            // then
            assert!(
                !project_dir.join("target/ink/new_project.contract").exists(),
                "found contract artifact in project directory!"
            );
            assert!(
                !project_dir.join("target/ink/new_project.wasm").exists(),
                "found wasm artifact in project directory!"
            );
            Ok(())
        })
    }
}

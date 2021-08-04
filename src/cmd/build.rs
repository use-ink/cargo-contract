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

use crate::{
    crate_metadata::CrateMetadata,
    maybe_println, util, validate_wasm,
    workspace::{Manifest, ManifestPath, Profile, Workspace},
    BuildArtifacts, BuildMode, BuildResult, OptimizationPasses, OptimizationResult, UnstableFlags,
    UnstableOptions, Verbosity, VerbosityFlags,
};
use anyhow::{Context, Result};
use colored::Colorize;
use parity_wasm::elements::{External, Internal, MemoryType, Module, Section};
use regex::Regex;
use semver::Version;
use std::{
    convert::TryFrom,
    ffi::OsStr,
    fs::metadata,
    path::{Path, PathBuf},
    process::Command,
    str,
};
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
    /// By default the contract is compiled with debug functionality
    /// included. This enables the contract to output debug messages,
    /// but increases the contract size and the amount of gas used.
    ///
    /// A production contract should always be build in `release` mode!
    /// Then no debug functionality is compiled into the contract.
    #[structopt(long = "--release")]
    build_release: bool,
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
    /// Number of optimization passes, passed as an argument to wasm-opt.
    ///
    /// - `0`: execute no optimization passes
    ///
    /// - `1`: execute 1 optimization pass (quick & useful opts, useful for iteration builds)
    ///
    /// - `2`, execute 2 optimization passes (most opts, generally gets most perf)
    ///
    /// - `3`, execute 3 optimization passes (spends potentially a lot of time optimizing)
    ///
    /// - `4`, execute 4 optimization passes (also flatten the IR, which can take a lot more time and memory
    /// but is useful on more nested / complex / less-optimized input)
    ///
    /// - `s`, execute default optimization passes, focusing on code size
    ///
    /// - `z`, execute default optimization passes, super-focusing on code size
    ///
    /// - The default value is `z`
    ///
    /// - It is possible to define the number of optimization passes in the
    ///   `[package.metadata.contract]` of your `Cargo.toml` as e.g. `optimization-passes = "3"`.
    ///   The CLI argument always takes precedence over the profile value.
    #[structopt(long)]
    optimization_passes: Option<OptimizationPasses>,
    /// Do not remove symbols (Wasm name section) when optimizing.
    ///
    /// This is useful if one wants to analyze or debug the optimized binary.
    #[structopt(long)]
    keep_debug_symbols: bool,
}

impl BuildCommand {
    pub fn exec(&self) -> Result<BuildResult> {
        let manifest_path = ManifestPath::try_from(self.manifest_path.as_ref())?;
        let unstable_flags: UnstableFlags =
            TryFrom::<&UnstableOptions>::try_from(&self.unstable_options)?;
        let verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;

        // The CLI flag `optimization-passes` overwrites optimization passes which are
        // potentially defined in the `Cargo.toml` profile.
        let optimization_passes = match self.optimization_passes {
            Some(opt_passes) => opt_passes,
            None => {
                let mut manifest = Manifest::new(manifest_path.clone())?;
                match manifest.get_profile_optimization_passes() {
                    // if no setting is found, neither on the cli nor in the profile,
                    // then we use the default
                    None => OptimizationPasses::default(),
                    Some(opt_passes) => opt_passes,
                }
            }
        };

        let build_mode = match self.build_release {
            true => BuildMode::Release,
            false => BuildMode::Debug,
        };
        execute(
            &manifest_path,
            verbosity,
            build_mode,
            self.build_artifact,
            unstable_flags,
            optimization_passes,
            self.keep_debug_symbols,
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
            BuildMode::Debug,
            BuildArtifacts::CheckOnly,
            unstable_flags,
            OptimizationPasses::Zero,
            false,
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
    build_mode: BuildMode,
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
        let target_dir = format!("--target-dir={}", target_dir.to_string_lossy());
        let mut args = vec![
            "--target=wasm32-unknown-unknown",
            "-Zbuild-std",
            "-Zbuild-std-features=panic_immediate_abort",
            "--no-default-features",
            "--release",
            &target_dir,
        ];
        if build_mode == BuildMode::Debug {
            args.push("--features=ink_env/ink-debug");
        }
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
/// The name section is already stripped by `wasm-opt`.
fn strip_custom_sections(module: &mut Module) {
    module.sections_mut().retain(|section| match section {
        Section::Reloc(_) => false,
        Section::Custom(custom) if custom.name() != "name" => false,
        _ => true,
    })
}

/// A contract should export nothing but the "call" and "deploy" functions.
///
/// Any elements not referenced by these exports become orphaned and are removed by `wasm-opt`.
fn strip_exports(module: &mut Module) {
    if let Some(section) = module.export_section_mut() {
        section.entries_mut().retain(|entry| {
            matches!(entry.internal(), Internal::Function(_))
                && (entry.field() == "call" || entry.field() == "deploy")
        })
    }
}

/// Load and parse a wasm file from disk.
fn load_module<P: AsRef<Path>>(path: P) -> Result<Module> {
    let path = path.as_ref();
    parity_wasm::deserialize_file(path).context(format!(
        "Loading of wasm module at '{}' failed",
        path.display(),
    ))
}

/// Performs required post-processing steps on the wasm artifact.
fn post_process_wasm(crate_metadata: &CrateMetadata) -> Result<()> {
    // Deserialize wasm module from a file.
    let mut module =
        load_module(&crate_metadata.original_wasm).context("Loading of original wasm failed")?;

    strip_exports(&mut module);
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
fn optimize_wasm(
    crate_metadata: &CrateMetadata,
    optimization_passes: OptimizationPasses,
    keep_debug_symbols: bool,
) -> Result<OptimizationResult> {
    let mut dest_optimized = crate_metadata.dest_wasm.clone();
    dest_optimized.set_file_name(format!(
        "{}-opt.wasm",
        crate_metadata.contract_artifact_name
    ));
    let _ = do_optimization(
        crate_metadata.dest_wasm.as_os_str(),
        dest_optimized.as_os_str(),
        optimization_passes,
        keep_debug_symbols,
    )?;

    if !dest_optimized.exists() {
        return Err(anyhow::anyhow!(
            "Optimization failed, optimized wasm output file `{}` not found.",
            dest_optimized.display()
        ));
    }

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

/// Optimizes the Wasm supplied as `crate_metadata.dest_wasm` using
/// the `wasm-opt` binary.
///
/// The supplied `optimization_level` denotes the number of optimization passes,
/// resulting in potentially a lot of time spent optimizing.
///
/// If successful, the optimized wasm is written to `dest_optimized`.
fn do_optimization(
    dest_wasm: &OsStr,
    dest_optimized: &OsStr,
    optimization_level: OptimizationPasses,
    keep_debug_symbols: bool,
) -> Result<()> {
    // check `wasm-opt` is installed
    let which = which::which("wasm-opt");
    if which.is_err() {
        anyhow::bail!(
            "wasm-opt not found! Make sure the binary is in your PATH environment.\n\
            We use this tool to optimize the size of your contract's Wasm binary.\n\n\
            wasm-opt is part of the binaryen package. You can find detailed\n\
            installation instructions on https://github.com/WebAssembly/binaryen#tools.\n\n\

            There are ready-to-install packages for many platforms:\n\
            * Debian/Ubuntu: apt-get install binaryen\n\
            * Homebrew: brew install binaryen\n\
            * Arch Linux: pacman -S binaryen\n\
            * Windows: binary releases at https://github.com/WebAssembly/binaryen/releases"
                .to_string()
                .bright_yellow()
        );
    }
    let wasm_opt_path = which
        .as_ref()
        .expect("we just checked if which returned an err; qed")
        .as_path();
    log::info!("Path to wasm-opt executable: {}", wasm_opt_path.display());

    let _ = check_wasm_opt_version_compatibility(wasm_opt_path)?;

    log::info!(
        "Optimization level passed to wasm-opt: {}",
        optimization_level
    );
    let mut command = Command::new(wasm_opt_path);
    command
        .arg(dest_wasm)
        .arg(format!("-O{}", optimization_level))
        .arg("-o")
        .arg(dest_optimized)
        // the memory in our module is imported, `wasm-opt` needs to be told that
        // the memory is initialized to zeroes, otherwise it won't run the
        // memory-packing pre-pass.
        .arg("--zero-filled-memory");
    if keep_debug_symbols {
        command.arg("-g");
    }
    let output = command.output().map_err(|err| {
        anyhow::anyhow!(
            "Executing {} failed with {:?}",
            wasm_opt_path.display(),
            err
        )
    })?;

    if !output.status.success() {
        let err = str::from_utf8(&output.stderr)
            .expect("Cannot convert stderr output of wasm-opt to string")
            .trim();
        anyhow::bail!(
            "The wasm-opt optimization failed.\n\n\
            The error which wasm-opt returned was: \n{}",
            err
        );
    }
    Ok(())
}

/// Checks if the wasm-opt binary under `wasm_opt_path` returns a version
/// compatible with `cargo-contract`.
///
/// Currently this must be a version >= 99.
fn check_wasm_opt_version_compatibility(wasm_opt_path: &Path) -> Result<()> {
    let cmd = Command::new(wasm_opt_path)
        .arg("--version")
        .output()
        .map_err(|err| {
            anyhow::anyhow!(
                "Executing `{:?} --version` failed with {:?}",
                wasm_opt_path.display(),
                err
            )
        })?;
    if !cmd.status.success() {
        let err = str::from_utf8(&cmd.stderr)
            .expect("Cannot convert stderr output of wasm-opt to string")
            .trim();
        anyhow::bail!(
            "Getting version information from wasm-opt failed.\n\
            The error which wasm-opt returned was: \n{}",
            err
        );
    }

    // ```sh
    // $ wasm-opt --version
    // wasm-opt version 99 (version_99-79-gc12cc3f50)
    // ```
    let github_note = "\n\n\
        If you tried installing from your system package manager the best\n\
        way forward is to download a recent binary release directly:\n\n\
        https://github.com/WebAssembly/binaryen/releases\n\n\
        Make sure that the `wasm-opt` file from that release is in your `PATH`.";
    let version_stdout = str::from_utf8(&cmd.stdout)
        .expect("Cannot convert stdout output of wasm-opt to string")
        .trim();
    let re = Regex::new(r"wasm-opt version (\d+)").expect("invalid regex");
    let captures = re.captures(version_stdout).ok_or_else(|| {
        anyhow::anyhow!(
            "Unable to extract version information from '{}'.\n\
            Your wasm-opt version is most probably too old. Make sure you use a version >= 99.{}",
            version_stdout,
            github_note,
        )
    })?;
    let version_number: u32 = captures
        .get(1) // first capture group is at index 1
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Unable to extract version number from '{:?}'",
                version_stdout
            )
        })?
        .as_str()
        .parse()
        .map_err(|err| {
            anyhow::anyhow!(
                "Parsing version number failed with '{:?}' for '{:?}'",
                err,
                version_stdout
            )
        })?;

    log::info!(
        "The wasm-opt version output is '{}', which was parsed to '{}'",
        version_stdout,
        version_number
    );
    if version_number < 99 {
        anyhow::bail!(
            "Your wasm-opt version is {}, but we require a version >= 99.{}",
            version_number,
            github_note,
        );
    }
    Ok(())
}

/// Asserts that the contract's dependencies are compatible to the ones used in ink!.
///
/// This function utilizes `cargo tree`, which takes semver into consideration.
///
/// Hence this function only returns an `Err` if it is a proper mismatch according
/// to semantic versioning. This means that either:
///     - the major version mismatches, differences in the minor/patch version
///       are not considered incompatible.
///     - or if the version starts with zero (i.e. `0.y.z`) a mismatch in the minor
///       version is already considered incompatible.
fn assert_compatible_ink_dependencies(
    manifest_path: &ManifestPath,
    verbosity: Verbosity,
) -> Result<()> {
    for dependency in ["parity-scale-codec", "scale-info"].iter() {
        let args = ["-i", dependency, "--duplicates"];
        let _ = util::invoke_cargo("tree", &args, manifest_path.directory(), verbosity).map_err(
            |_| {
                anyhow::anyhow!(
                    "Mismatching versions of `{}` were found!\n\
                     Please ensure that your contract and your ink! dependencies use a compatible \
                     version of this package.",
                    dependency
                )
            },
        )?;
    }
    Ok(())
}

/// Checks whether the supplied `ink_version` already contains the debug feature.
///
/// This feature was introduced in `3.0.0-rc4` with `ink_env/ink-debug`.
pub fn assert_debug_mode_supported(ink_version: &Version) -> anyhow::Result<()> {
    log::info!("Contract version: {:?}", ink_version);
    let minimum_version = Version::parse("3.0.0-rc4").expect("parsing version failed");
    if ink_version < &minimum_version {
        anyhow::bail!(
            "Building the contract in debug mode requires an ink! version newer than `3.0.0-rc3`!"
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
    build_mode: BuildMode,
    build_artifact: BuildArtifacts,
    unstable_flags: UnstableFlags,
    optimization_passes: OptimizationPasses,
    keep_debug_symbols: bool,
) -> Result<BuildResult> {
    let crate_metadata = CrateMetadata::collect(manifest_path)?;

    assert_compatible_ink_dependencies(manifest_path, verbosity)?;
    if build_mode == BuildMode::Debug {
        assert_debug_mode_supported(&crate_metadata.ink_version)?;
    }

    let build = || -> Result<OptimizationResult> {
        maybe_println!(
            verbosity,
            " {} {}",
            format!("[1/{}]", build_artifact.steps()).bold(),
            "Building cargo project".bright_green().bold()
        );
        exec_cargo_for_wasm_target(
            &crate_metadata,
            "build",
            build_mode,
            verbosity,
            &unstable_flags,
        )?;

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
        let optimization_result =
            optimize_wasm(&crate_metadata, optimization_passes, keep_debug_symbols)?;

        Ok(optimization_result)
    };

    let (opt_result, metadata_result) = match build_artifact {
        BuildArtifacts::CheckOnly => {
            exec_cargo_for_wasm_target(
                &crate_metadata,
                "check",
                BuildMode::Release,
                verbosity,
                &unstable_flags,
            )?;
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
        build_mode,
        build_artifact,
        verbosity,
    })
}

#[cfg(feature = "test-ci-only")]
#[cfg(test)]
mod tests_ci_only {
    use super::{
        assert_compatible_ink_dependencies, assert_debug_mode_supported,
        check_wasm_opt_version_compatibility,
    };
    use crate::{
        cmd::{build::load_module, BuildCommand},
        util::tests::{with_new_contract_project, with_tmp_dir},
        workspace::Manifest,
        BuildArtifacts, BuildMode, ManifestPath, OptimizationPasses, UnstableFlags,
        UnstableOptions, Verbosity, VerbosityFlags,
    };
    use semver::Version;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::{
        ffi::OsStr,
        io::Write,
        path::{Path, PathBuf},
    };

    /// Modifies the `Cargo.toml` under the supplied `cargo_toml_path` by
    /// setting `optimization-passes` in `[package.metadata.contract]` to `passes`.
    fn write_optimization_passes_into_manifest(cargo_toml_path: &Path, passes: OptimizationPasses) {
        let manifest_path =
            ManifestPath::new(cargo_toml_path).expect("manifest path creation failed");
        let mut manifest = Manifest::new(manifest_path.clone()).expect("manifest creation failed");
        manifest
            .set_profile_optimization_passes(passes)
            .expect("setting `optimization-passes` in profile failed");
        manifest
            .write(&manifest_path)
            .expect("writing manifest failed");
    }

    fn has_debug_symbols<P: AsRef<Path>>(p: P) -> bool {
        load_module(p)
            .unwrap()
            .custom_sections()
            .any(|e| e.name() == "name")
    }

    /// Creates an executable `wasm-opt-mocked` file which outputs
    /// "wasm-opt version `version`".
    ///
    /// Returns the path to this file.
    ///
    /// Currently works only on `unix`.
    #[cfg(unix)]
    fn mock_wasm_opt_version(tmp_dir: &Path, version: &str) -> PathBuf {
        let path = tmp_dir.join("wasm-opt-mocked");
        {
            let mut file = std::fs::File::create(&path).unwrap();
            let version = format!("#!/bin/sh\necho \"wasm-opt version {}\"", version);
            file.write_all(version.as_bytes())
                .expect("writing wasm-opt-mocked failed");
        }
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o777))
            .expect("setting permissions failed");
        path
    }

    #[test]
    fn build_code_only() {
        with_new_contract_project(|manifest_path| {
            let res = super::execute(
                &manifest_path,
                Verbosity::Default,
                BuildMode::default(),
                BuildArtifacts::CodeOnly,
                UnstableFlags::default(),
                OptimizationPasses::default(),
                false,
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

            // we specified that debug symbols should be removed
            // original code should have some but the optimized version should have them removed
            assert!(!has_debug_symbols(&res.dest_wasm.unwrap()));

            Ok(())
        })
    }

    #[test]
    fn check_must_not_output_contract_artifacts_in_project_dir() {
        with_new_contract_project(|manifest_path| {
            // given
            let project_dir = manifest_path.directory().expect("directory must exist");

            // when
            super::execute(
                &manifest_path,
                Verbosity::Default,
                BuildMode::default(),
                BuildArtifacts::CheckOnly,
                UnstableFlags::default(),
                OptimizationPasses::default(),
                false,
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

    #[test]
    fn optimization_passes_from_cli_must_take_precedence_over_profile() {
        with_new_contract_project(|manifest_path| {
            // given
            write_optimization_passes_into_manifest(
                manifest_path.as_ref(),
                OptimizationPasses::Three,
            );
            let cmd = BuildCommand {
                manifest_path: Some(manifest_path.into()),
                build_artifact: BuildArtifacts::All,
                build_release: false,
                verbosity: VerbosityFlags::default(),
                unstable_options: UnstableOptions::default(),

                // we choose zero optimization passes as the "cli" parameter
                optimization_passes: Some(OptimizationPasses::Zero),
                keep_debug_symbols: false,
            };

            // when
            let res = cmd.exec().expect("build failed");
            let optimization = res
                .optimization_result
                .expect("no optimization result available");

            // then
            // The size does not exactly match the original size even without optimization
            // passed because there is still some post processing happening.
            let size_diff = optimization.original_size - optimization.optimized_size;
            assert!(
                0.0 < size_diff && size_diff < 10.0,
                "The optimized size savings are larger than allowed or negative: {}",
                size_diff,
            );
            Ok(())
        })
    }

    #[test]
    fn optimization_passes_from_profile_must_be_used() {
        with_new_contract_project(|manifest_path| {
            // given
            write_optimization_passes_into_manifest(
                manifest_path.as_ref(),
                OptimizationPasses::Three,
            );
            let cmd = BuildCommand {
                manifest_path: Some(manifest_path.into()),
                build_artifact: BuildArtifacts::All,
                build_release: false,
                verbosity: VerbosityFlags::default(),
                unstable_options: UnstableOptions::default(),

                // we choose no optimization passes as the "cli" parameter
                optimization_passes: None,
                keep_debug_symbols: false,
            };

            // when
            let res = cmd.exec().expect("build failed");
            let optimization = res
                .optimization_result
                .expect("no optimization result available");

            // then
            // The size does not exactly match the original size even without optimization
            // passed because there is still some post processing happening.
            let size_diff = optimization.original_size - optimization.optimized_size;
            assert!(
                size_diff > (optimization.original_size / 2.0),
                "The optimized size savings are too small: {}",
                size_diff,
            );

            Ok(())
        })
    }

    #[test]
    fn project_template_dependencies_must_be_ink_compatible() {
        with_new_contract_project(|manifest_path| {
            // given
            // the manifest path

            // when
            let res = assert_compatible_ink_dependencies(&manifest_path, Verbosity::Default);

            // then
            assert!(res.is_ok());
            Ok(())
        })
    }

    #[test]
    fn detect_mismatching_parity_scale_codec_dependencies() {
        with_new_contract_project(|manifest_path| {
            // given
            // the manifest path

            // at the time of writing this test ink! already uses `parity-scale-codec`
            // in a version > 2, hence 1 is an incompatible version.
            let mut manifest = Manifest::new(manifest_path.clone())?;
            manifest
                .set_dependency_version("scale", "1.0.0")
                .expect("setting `scale` version failed");
            manifest
                .write(&manifest_path)
                .expect("writing manifest failed");

            // when
            let res = assert_compatible_ink_dependencies(&manifest_path, Verbosity::Default);

            // then
            assert!(res.is_err());
            Ok(())
        })
    }

    #[cfg(unix)]
    #[test]
    fn incompatible_wasm_opt_version_must_be_detected_if_built_from_repo() {
        with_tmp_dir(|path| {
            // given
            let path = mock_wasm_opt_version(path, "98 (version_13-79-gc12cc3f50)");

            // when
            let res = check_wasm_opt_version_compatibility(&path);

            // then
            assert!(res.is_err());
            assert!(
                format!("{:?}", res).starts_with(
                    "Err(Your wasm-opt version is 98, but we require a version >= 99."
                ),
                "Expected a different output, found {:?}",
                res
            );

            Ok(())
        })
    }

    #[cfg(unix)]
    #[test]
    fn compatible_wasm_opt_version_must_be_detected_if_built_from_repo() {
        with_tmp_dir(|path| {
            // given
            let path = mock_wasm_opt_version(path, "99 (version_99-79-gc12cc3f50");

            // when
            let res = check_wasm_opt_version_compatibility(&path);

            // then
            assert!(res.is_ok());

            Ok(())
        })
    }

    #[cfg(unix)]
    #[test]
    fn incompatible_wasm_opt_version_must_be_detected_if_installed_as_package() {
        with_tmp_dir(|path| {
            // given
            let path = mock_wasm_opt_version(path, "98");

            // when
            let res = check_wasm_opt_version_compatibility(&path);

            // then
            assert!(res.is_err());
            assert!(format!("{:?}", res)
                .starts_with("Err(Your wasm-opt version is 98, but we require a version >= 99."));

            Ok(())
        })
    }

    #[cfg(unix)]
    #[test]
    fn compatible_wasm_opt_version_must_be_detected_if_installed_as_package() {
        with_tmp_dir(|path| {
            // given
            let path = mock_wasm_opt_version(path, "99");

            // when
            let res = check_wasm_opt_version_compatibility(&path);

            // then
            assert!(res.is_ok());

            Ok(())
        })
    }

    #[test]
    fn contract_lib_name_different_from_package_name_must_build() {
        with_new_contract_project(|manifest_path| {
            // given
            let mut manifest =
                Manifest::new(manifest_path.clone()).expect("manifest creation failed");
            let _ = manifest
                .set_lib_name("some_lib_name")
                .expect("setting lib name failed");
            let _ = manifest
                .set_package_name("some_package_name")
                .expect("setting pacakge name failed");
            manifest
                .write(&manifest_path)
                .expect("writing manifest failed");

            // when
            let cmd = BuildCommand {
                manifest_path: Some(manifest_path.into()),
                build_artifact: BuildArtifacts::All,
                build_release: false,
                verbosity: VerbosityFlags::default(),
                unstable_options: UnstableOptions::default(),
                optimization_passes: None,
                keep_debug_symbols: false,
            };
            let res = cmd.exec().expect("build failed");

            // then
            assert_eq!(
                res.dest_wasm
                    .expect("`dest_wasm` does not exist")
                    .file_name(),
                Some(OsStr::new("some_lib_name.wasm"))
            );

            Ok(())
        })
    }

    #[test]
    pub fn debug_mode_must_be_compatible() {
        let _ =
            assert_debug_mode_supported(&Version::parse("3.0.0-rc4").expect("parsing must work"))
                .expect("debug mode must be compatible");
        let _ =
            assert_debug_mode_supported(&Version::parse("4.0.0-rc1").expect("parsing must work"))
                .expect("debug mode must be compatible");
        let _ = assert_debug_mode_supported(&Version::parse("5.0.0").expect("parsing must work"))
            .expect("debug mode must be compatible");
    }

    #[test]
    pub fn debug_mode_must_be_incompatible() {
        let res =
            assert_debug_mode_supported(&Version::parse("3.0.0-rc3").expect("parsing must work"))
                .expect_err("assertion must fail");
        assert_eq!(
            res.to_string(),
            "Building the contract in debug mode requires an ink! version newer than `3.0.0-rc3`!"
        );
    }

    #[test]
    fn building_template_in_debug_mode_must_work() {
        with_new_contract_project(|manifest_path| {
            // given
            let build_mode = BuildMode::Debug;

            // when
            let res = super::execute(
                &manifest_path,
                Verbosity::Default,
                build_mode,
                BuildArtifacts::All,
                UnstableFlags::default(),
                OptimizationPasses::default(),
                Default::default(),
            );

            // then
            assert!(res.is_ok(), "building template in debug mode failed!");
            Ok(())
        })
    }

    #[test]
    fn building_template_in_release_mode_must_work() {
        with_new_contract_project(|manifest_path| {
            // given
            let build_mode = BuildMode::Release;

            // when
            let res = super::execute(
                &manifest_path,
                Verbosity::Default,
                build_mode,
                BuildArtifacts::All,
                UnstableFlags::default(),
                OptimizationPasses::default(),
                Default::default(),
            );

            // then
            assert!(res.is_ok(), "building template in release mode failed!");
            Ok(())
        })
    }

    #[test]
    fn building_contract_with_source_file_in_subfolder_must_work() {
        with_new_contract_project(|manifest_path| {
            // given
            let path = manifest_path.directory().expect("dir must exist");
            let old_lib_path = path.join(Path::new("lib.rs"));
            let new_lib_path = path.join(Path::new("srcfoo")).join(Path::new("lib.rs"));
            let new_dir_path = path.join(Path::new("srcfoo"));
            std::fs::create_dir_all(new_dir_path).expect("creating dir must work");
            std::fs::rename(old_lib_path, new_lib_path).expect("moving file must work");

            let mut manifest =
                Manifest::new(manifest_path.clone()).expect("creating manifest must work");
            manifest
                .set_lib_path("srcfoo/lib.rs")
                .expect("setting lib path must work");
            manifest.write(&manifest_path).expect("writing must work");

            // when
            let res = super::execute(
                &manifest_path,
                Verbosity::Default,
                BuildMode::default(),
                BuildArtifacts::CheckOnly,
                UnstableFlags::default(),
                OptimizationPasses::default(),
                Default::default(),
            );

            // then
            assert!(res.is_ok(), "building contract failed!");
            Ok(())
        })
    }

    #[test]
    fn keep_debug_symbols_in_debug_mode() {
        with_new_contract_project(|manifest_path| {
            let res = super::execute(
                &manifest_path,
                Verbosity::Default,
                BuildMode::Debug,
                BuildArtifacts::CodeOnly,
                UnstableFlags::default(),
                OptimizationPasses::default(),
                true,
            )
            .expect("build failed");

            // we specified that debug symbols should be kept
            assert!(has_debug_symbols(&res.dest_wasm.unwrap()));

            Ok(())
        })
    }

    #[test]
    fn keep_debug_symbols_in_release_mode() {
        with_new_contract_project(|manifest_path| {
            let res = super::execute(
                &manifest_path,
                Verbosity::Default,
                BuildMode::Release,
                BuildArtifacts::CodeOnly,
                UnstableFlags::default(),
                OptimizationPasses::default(),
                true,
            )
            .expect("build failed");

            // we specified that debug symbols should be kept
            assert!(has_debug_symbols(&res.dest_wasm.unwrap()));

            Ok(())
        })
    }
}

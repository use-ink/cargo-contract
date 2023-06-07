// Copyright 2018-2022 Parity Technologies (UK) Ltd.
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

#![doc = include_str!("../README.md")]
#![deny(unused_crate_dependencies)]

use which as _;

mod args;
mod crate_metadata;
mod docker;
pub mod metadata;
mod new;
#[cfg(test)]
mod tests;
pub mod util;
mod validate_wasm;
mod wasm_opt;
mod workspace;

#[deprecated(since = "2.0.2", note = "Use MetadataArtifacts instead")]
pub use self::metadata::MetadataArtifacts as MetadataResult;

pub use self::{
    args::{
        BuildArtifacts,
        BuildMode,
        BuildSteps,
        Features,
        Network,
        OutputType,
        Target,
        UnstableFlags,
        UnstableOptions,
        Verbosity,
        VerbosityFlags,
    },
    crate_metadata::CrateMetadata,
    metadata::{
        BuildInfo,
        MetadataArtifacts,
        WasmOptSettings,
    },
    new::new_contract_project,
    util::DEFAULT_KEY_COL_WIDTH,
    wasm_opt::{
        OptimizationPasses,
        OptimizationResult,
    },
    workspace::{
        Manifest,
        ManifestPath,
        Profile,
        Workspace,
    },
};

use crate::wasm_opt::WasmOptHandler;
pub use docker::docker_build;

use anyhow::{
    Context,
    Result,
};
use colored::Colorize;
use parity_wasm::elements::{
    External,
    Internal,
    MemoryType,
    Module,
    Section,
};
use semver::Version;
use std::{
    collections::VecDeque,
    fs,
    io,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
    str,
};
use strum::IntoEnumIterator;

/// This is the default maximum number of pages available for a contract to allocate.
pub const DEFAULT_MAX_MEMORY_PAGES: u32 = 16;

/// Version of the currently executing `cargo-contract` binary.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Arguments to use when executing `build` or `check` commands.
#[derive(Clone)]
pub struct ExecuteArgs {
    /// The location of the Cargo manifest (`Cargo.toml`) file to use.
    pub manifest_path: ManifestPath,
    pub verbosity: Verbosity,
    pub build_mode: BuildMode,
    pub features: Features,
    pub network: Network,
    pub build_artifact: BuildArtifacts,
    pub unstable_flags: UnstableFlags,
    pub optimization_passes: Option<OptimizationPasses>,
    pub keep_debug_symbols: bool,
    pub lint: bool,
    pub output_type: OutputType,
    pub skip_wasm_validation: bool,
    pub target: Target,
    pub max_memory_pages: u32,
}

impl Default for ExecuteArgs {
    fn default() -> Self {
        Self {
            manifest_path: Default::default(),
            verbosity: Default::default(),
            build_mode: Default::default(),
            features: Default::default(),
            network: Default::default(),
            build_artifact: Default::default(),
            unstable_flags: Default::default(),
            optimization_passes: Default::default(),
            keep_debug_symbols: Default::default(),
            lint: Default::default(),
            output_type: Default::default(),
            skip_wasm_validation: Default::default(),
            target: Default::default(),
            max_memory_pages: DEFAULT_MAX_MEMORY_PAGES,
        }
    }
}

/// Result of the build process.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct BuildResult {
    /// Path to the resulting Wasm file.
    pub dest_wasm: Option<PathBuf>,
    /// Result of the metadata generation.
    pub metadata_result: Option<MetadataArtifacts>,
    /// Path to the directory where output files are written to.
    pub target_directory: PathBuf,
    /// If existent the result of the optimization.
    pub optimization_result: Option<OptimizationResult>,
    /// The mode to build the contract in.
    pub build_mode: BuildMode,
    /// Which build artifacts were generated.
    pub build_artifact: BuildArtifacts,
    /// The verbosity flags.
    pub verbosity: Verbosity,
    /// Image used for the verifiable build
    pub image: Option<String>,
    /// The type of formatting to use for the build output.
    #[serde(skip_serializing, skip_deserializing)]
    pub output_type: OutputType,
}

impl BuildResult {
    pub fn display(&self) -> String {
        let opt_size_diff = if let Some(ref opt_result) = self.optimization_result {
            let size_diff = format!(
                "\nOriginal wasm size: {}, Optimized: {}\n\n",
                format!("{:.1}K", opt_result.original_size).bold(),
                format!("{:.1}K", opt_result.optimized_size).bold(),
            );
            debug_assert!(
                opt_result.optimized_size > 0.0,
                "optimized file size must be greater 0"
            );
            size_diff
        } else {
            "\n".to_string()
        };

        let build_mode = format!(
            "The contract was built in {} mode.\n\n",
            format!("{}", self.build_mode).to_uppercase().bold(),
        );

        if self.build_artifact == BuildArtifacts::CodeOnly {
            let out = format!(
                "{}{}Your contract's code is ready. You can find it here:\n{}",
                opt_size_diff,
                build_mode,
                self.dest_wasm
                    .as_ref()
                    .expect("wasm path must exist")
                    .display()
                    .to_string()
                    .bold()
            );
            return out
        };

        let mut out = format!(
            "{}{}Your contract artifacts are ready. You can find them in:\n{}\n\n",
            opt_size_diff,
            build_mode,
            self.target_directory.display().to_string().bold(),
        );
        if let Some(metadata_result) = self.metadata_result.as_ref() {
            let bundle = format!(
                "  - {} (code + metadata)\n",
                util::base_name(&metadata_result.dest_bundle).bold()
            );
            out.push_str(&bundle);
        }
        if let Some(dest_wasm) = self.dest_wasm.as_ref() {
            let wasm = format!(
                "  - {} (the contract's code)\n",
                util::base_name(dest_wasm).bold()
            );
            out.push_str(&wasm);
        }
        if let Some(metadata_result) = self.metadata_result.as_ref() {
            let metadata = format!(
                "  - {} (the contract's metadata)",
                util::base_name(&metadata_result.dest_metadata).bold()
            );
            out.push_str(&metadata);
        }
        out
    }

    /// Display the build results in a pretty formatted JSON string.
    pub fn serialize_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

/// Executes the supplied cargo command on the project in the specified directory,
/// defaults to the current directory.
///
/// Uses the unstable cargo feature [`build-std`](https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#build-std)
/// to build the standard library with [`panic_immediate_abort`](https://github.com/johnthagen/min-sized-rust#remove-panic-string-formatting-with-panic_immediate_abort)
/// which reduces the size of the Wasm binary by not including panic strings and
/// formatting code.
///
/// # `Cargo.toml` optimizations
///
/// The original `Cargo.toml` will be amended to remove the `rlib` crate type in order to
/// minimize the final Wasm binary size.
///
/// Preferred default `[profile.release]` settings will be added if they are missing,
/// existing user-defined settings will be preserved.
///
/// The `[workspace]` will be added if it is missing to ignore `workspace` from parent
/// `Cargo.toml`.
///
/// To disable this and use the original `Cargo.toml` as is then pass the `-Z
/// original_manifest` flag.
#[allow(clippy::too_many_arguments)]
fn exec_cargo_for_onchain_target(
    crate_metadata: &CrateMetadata,
    command: &str,
    features: &Features,
    build_mode: &BuildMode,
    network: &Network,
    verbosity: &Verbosity,
    unstable_flags: &UnstableFlags,
    target: &Target,
) -> Result<()> {
    let cargo_build = |manifest_path: &ManifestPath| {
        let target_dir = format!(
            "--target-dir={}",
            crate_metadata.target_directory.to_string_lossy()
        );

        let mut args = vec![
            format!("--target={}", target.llvm_target()),
            "-Zbuild-std=core,alloc".to_owned(),
            "--no-default-features".to_owned(),
            "--release".to_owned(),
            target_dir,
        ];
        network.append_to_args(&mut args);

        let mut features = features.clone();
        if build_mode == &BuildMode::Debug {
            features.push("ink/ink-debug");
        } else {
            args.push("-Zbuild-std-features=panic_immediate_abort".to_owned());
        }
        features.append_to_args(&mut args);
        let mut env = Vec::new();
        if rustc_version::version_meta()?.channel == rustc_version::Channel::Stable {
            // Allow nightly features on a stable toolchain
            env.push(("RUSTC_BOOTSTRAP", Some("1".to_string())))
        }
        // the linker needs our linker script as file
        let rustflags = target.rustflags();
        if matches!(target, Target::RiscV) {
            fs::create_dir_all(&crate_metadata.target_directory)?;
            let path = crate_metadata
                .target_directory
                .join(".riscv_memory_layout.ld");
            fs::write(&path, include_bytes!("../riscv_memory_layout.ld"))?;
            let path = path.display();
            env.push((
                "CARGO_ENCODED_RUSTFLAGS",
                Some(format!("{rustflags} -Clink-arg=-T{path}",)),
            ));
            Some(path)
        } else {
            env.push(("CARGO_ENCODED_RUSTFLAGS", Some(rustflags.to_string())));
            None
        };

        let cargo =
            util::cargo_cmd(command, &args, manifest_path.directory(), *verbosity, env);

        invoke_cargo_and_scan_for_error(cargo)
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
                    .with_replaced_lib_to_bin()?
                    .with_profile_release_defaults(Profile::default_contract_release())?
                    .with_empty_workspace();
                Ok(())
            })?
            .using_temp(cargo_build)?;
    }

    Ok(())
}

/// Executes the supplied cargo command, reading the output and scanning for known errors.
/// Writes the captured stderr back to stderr and maintains the cargo tty progress bar.
fn invoke_cargo_and_scan_for_error(cargo: duct::Expression) -> Result<()> {
    macro_rules! eprintln_red {
        ($value:expr) => {{
            use colored::Colorize as _;
            ::std::eprintln!("{}", $value.bright_red().bold());
        }};
    }

    let cargo = util::cargo_tty_output(cargo);

    let missing_main_err = "error[E0601]".as_bytes();
    let mut err_buf = VecDeque::with_capacity(missing_main_err.len());

    let mut reader = cargo.stderr_to_stdout().reader()?;
    let mut buffer = [0u8; 1];

    loop {
        let bytes_read = io::Read::read(&mut reader, &mut buffer)?;
        for byte in buffer[0..bytes_read].iter() {
            err_buf.push_back(*byte);
            if err_buf.len() > missing_main_err.len() {
                let byte = err_buf.pop_front().expect("buffer is not empty");
                io::Write::write(&mut io::stderr(), &[byte])?;
            }
        }
        if missing_main_err == err_buf.make_contiguous() {
            eprintln_red!("\nExited with error: [E0601]");
            eprintln_red!(
                "Your contract must be annotated with the `no_main` attribute.\n"
            );
            eprintln_red!("Examples how to do this:");
            eprintln_red!("   - `#![cfg_attr(not(feature = \"std\"), no_std, no_main)]`");
            eprintln_red!("   - `#[no_main]`\n");
            return Err(anyhow::anyhow!("missing `no_main` attribute"))
        }
        if bytes_read == 0 {
            // flush the remaining buffered bytes
            io::Write::write(&mut io::stderr(), err_buf.make_contiguous())?;
            break
        }
        buffer = [0u8; 1];
    }
    Ok(())
}

/// Executes `cargo dylint` with the ink! linting driver that is built during
/// the `build.rs`.
///
/// We create a temporary folder, extract the linting driver there and run
/// `cargo dylint` with it.
fn exec_cargo_dylint(crate_metadata: &CrateMetadata, verbosity: Verbosity) -> Result<()> {
    check_dylint_requirements(crate_metadata.manifest_path.directory())?;

    // `dylint` is verbose by default, it doesn't have a `--verbose` argument,
    let verbosity = match verbosity {
        Verbosity::Verbose => Verbosity::Default,
        Verbosity::Default | Verbosity::Quiet => Verbosity::Quiet,
    };

    let target_dir = &crate_metadata.target_directory.to_string_lossy();
    let args = vec!["--lib=ink_linting"];
    let env = vec![
        // We need to set the `CARGO_TARGET_DIR` environment variable in
        // case `cargo dylint` is invoked.
        //
        // This is because we build from a temporary directory (to patch the manifest)
        // but still want the output to live at a fixed path. `cargo dylint` does
        // not accept this information on the command line.
        ("CARGO_TARGET_DIR", Some(target_dir.to_string())),
        // There are generally problems with having a custom `rustc` wrapper, while
        // executing `dylint` (which has a custom linker). Especially for `sccache`
        // there is this bug: https://github.com/mozilla/sccache/issues/1000.
        // Until we have a justification for leaving the wrapper we should unset it.
        ("RUSTC_WRAPPER", None),
    ];

    Workspace::new(&crate_metadata.cargo_meta, &crate_metadata.root_package.id)?
        .with_root_package_manifest(|manifest| {
            manifest.with_dylint()?.with_empty_workspace();
            Ok(())
        })?
        .using_temp(|manifest_path| {
            let cargo = util::cargo_cmd(
                "dylint",
                &args,
                manifest_path.directory(),
                verbosity,
                env,
            );
            cargo.run()?;
            Ok(())
        })?;

    Ok(())
}

/// Checks if all requirements for `dylint` are installed.
///
/// We require both `cargo-dylint` and `dylint-link` because the driver is being
/// built at runtime on demand.
///
/// This function takes a `_working_dir` which is only used for unit tests.
fn check_dylint_requirements(_working_dir: Option<&Path>) -> Result<()> {
    let execute_cmd = |cmd: &mut Command| {
        let mut child = if let Ok(child) = cmd
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            child
        } else {
            tracing::debug!("Error spawning `{:?}`", cmd);
            return false
        };

        child.wait().map(|ret| ret.success()).unwrap_or_else(|err| {
            tracing::debug!("Error waiting for `{:?}`: {:?}", cmd, err);
            false
        })
    };

    // when testing this function we should never fall back to a `cargo` specified
    // in the env variable, as this would mess with the mocked binaries.
    #[cfg(not(test))]
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    #[cfg(test)]
    let cargo = "cargo";

    if !execute_cmd(Command::new(cargo).arg("dylint").arg("--version")) {
        anyhow::bail!("cargo-dylint was not found!\n\
            Make sure it is installed and the binary is in your PATH environment.\n\n\
            You can install it by executing `cargo install cargo-dylint`."
            .to_string()
            .bright_yellow());
    }

    // On windows we cannot just run the linker with --version as there is no command
    // which just outputs some information. It always needs to do some linking in
    // order to return successful exit code.
    #[cfg(windows)]
    let dylint_link_found = which::which("dylint-link").is_ok();
    #[cfg(not(windows))]
    let dylint_link_found = execute_cmd(Command::new("dylint-link").arg("--version"));
    if !dylint_link_found {
        anyhow::bail!("dylint-link was not found!\n\
            Make sure it is installed and the binary is in your PATH environment.\n\n\
            You can install it by executing `cargo install dylint-link`."
            .to_string()
            .bright_yellow());
    }

    Ok(())
}

/// Ensures the Wasm memory import of a given module has the maximum number of pages.
///
/// Iterates over the import section, finds the memory import entry if any and adjusts the
/// maximum limit.
fn ensure_maximum_memory_pages(
    module: &mut Module,
    maximum_allowed_pages: u32,
) -> Result<()> {
    let mem_ty = module
        .import_section_mut()
        .and_then(|section| {
            section.entries_mut().iter_mut().find_map(|entry| {
                match entry.external_mut() {
                    External::Memory(ref mut mem_ty) => Some(mem_ty),
                    _ => None,
                }
            })
        })
        .context(
            "Memory import is not found. Is --import-memory specified in the linker args",
        )?;

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
        *mem_ty = MemoryType::new(initial, Some(maximum_allowed_pages));
    }

    Ok(())
}

/// Strips all custom sections.
///
/// Presently all custom sections are not required so they can be stripped safely.
/// The name section is already stripped by `wasm-opt`.
fn strip_custom_sections(module: &mut Module) {
    module.sections_mut().retain(|section| {
        match section {
            Section::Reloc(_) => false,
            Section::Custom(custom) if custom.name() != "name" => false,
            _ => true,
        }
    })
}

/// A contract should export nothing but the "call" and "deploy" functions.
///
/// Any elements not referenced by these exports become orphaned and are removed by
/// `wasm-opt`.
fn strip_exports(module: &mut Module) {
    if let Some(section) = module.export_section_mut() {
        section.entries_mut().retain(|entry| {
            matches!(entry.internal(), Internal::Function(_))
                && (entry.field() == "call" || entry.field() == "deploy")
        })
    }
}

/// Load and parse a Wasm file from disk.
fn load_module<P: AsRef<Path>>(path: P) -> Result<Module> {
    let path = path.as_ref();
    parity_wasm::deserialize_file(path).context(format!(
        "Loading of wasm module at '{}' failed",
        path.display(),
    ))
}

/// Performs required post-processing steps on the Wasm artifact.
fn post_process_wasm(
    crate_metadata: &CrateMetadata,
    skip_wasm_validation: bool,
    verbosity: &Verbosity,
    max_memory_pages: u32,
) -> Result<()> {
    // Deserialize Wasm module from a file.
    let mut module = load_module(&crate_metadata.original_code)
        .context("Loading of original wasm failed")?;

    strip_exports(&mut module);
    ensure_maximum_memory_pages(&mut module, max_memory_pages)?;
    strip_custom_sections(&mut module);

    if !skip_wasm_validation {
        validate_wasm::validate_import_section(&module)?;
    } else {
        maybe_println!(
            verbosity,
            " {}",
            "Skipping wasm validation! Contract code may be invalid."
                .bright_yellow()
                .bold()
        );
    }

    debug_assert!(
        !module.clone().into_bytes().unwrap().is_empty(),
        "resulting wasm size of post processing must be > 0"
    );

    parity_wasm::serialize_to_file(&crate_metadata.dest_code, module)?;
    Ok(())
}

/// Asserts that the contract's dependencies are compatible to the ones used in ink!.
///
/// This function utilizes `cargo tree`, which takes semver into consideration.
///
/// Hence this function only returns an `Err` if it is a proper mismatch according
/// to semantic versioning. This means that either:
///     - the major version mismatches, differences in the minor/patch version are not
///       considered incompatible.
///     - or if the version starts with zero (i.e. `0.y.z`) a mismatch in the minor
///       version is already considered incompatible.
fn assert_compatible_ink_dependencies(
    manifest_path: &ManifestPath,
    verbosity: Verbosity,
) -> Result<()> {
    for dependency in ["parity-scale-codec", "scale-info"].iter() {
        let args = ["-i", dependency, "--duplicates"];
        let cargo =
            util::cargo_cmd("tree", args, manifest_path.directory(), verbosity, vec![]);
        cargo
            .stdout_null()
            .run()
            .with_context(|| {
                format!(
                    "Mismatching versions of `{dependency}` were found!\n\
                     Please ensure that your contract and your ink! dependencies use a compatible \
                     version of this package."
                )
            })?;
    }
    Ok(())
}

/// Checks whether the supplied `ink_version` already contains the debug feature.
///
/// This feature was introduced in `3.0.0-rc4` with `ink_env/ink-debug`.
pub fn assert_debug_mode_supported(ink_version: &Version) -> anyhow::Result<()> {
    tracing::debug!("Contract version: {:?}", ink_version);
    let minimum_version = Version::parse("3.0.0-rc4").expect("parsing version failed");
    if ink_version < &minimum_version {
        anyhow::bail!(
            "Building the contract in debug mode requires an ink! version newer than `3.0.0-rc3`!"
        );
    }
    Ok(())
}

/// Executes build of the smart contract which produces a Wasm binary that is ready for
/// deploying.
///
/// It does so by invoking `cargo build` and then post processing the final binary.
pub fn execute(args: ExecuteArgs) -> Result<BuildResult> {
    let ExecuteArgs {
        manifest_path,
        verbosity,
        features,
        build_mode,
        network,
        build_artifact,
        unstable_flags,
        optimization_passes,
        lint,
        output_type,
        target,
        ..
    } = &args;

    // The CLI flag `optimization-passes` overwrites optimization passes which are
    // potentially defined in the `Cargo.toml` profile.
    let optimization_passes = match optimization_passes {
        Some(opt_passes) => *opt_passes,
        None => {
            let mut manifest = Manifest::new(manifest_path.clone())?;

            match manifest.profile_optimization_passes() {
                // if no setting is found, neither on the cli nor in the profile,
                // then we use the default
                None => OptimizationPasses::default(),
                Some(opt_passes) => opt_passes,
            }
        }
    };

    let crate_metadata = CrateMetadata::collect(manifest_path, *target)?;

    assert_compatible_ink_dependencies(manifest_path, *verbosity)?;
    if build_mode == &BuildMode::Debug {
        assert_debug_mode_supported(&crate_metadata.ink_version)?;
    }

    let clean_metadata = || {
        fs::remove_file(crate_metadata.metadata_path()).ok();
        fs::remove_file(crate_metadata.contract_bundle_path()).ok();
    };

    let (opt_result, metadata_result, dest_wasm) = match build_artifact {
        BuildArtifacts::CheckOnly => {
            let mut build_steps = BuildSteps::new();
            maybe_lint(
                &mut build_steps,
                *build_artifact,
                *lint,
                &crate_metadata,
                verbosity,
            )?;

            maybe_println!(
                verbosity,
                " {} {}",
                format!("{build_steps}").bold(),
                "Executing `cargo check`".bright_green().bold()
            );
            exec_cargo_for_onchain_target(
                &crate_metadata,
                "check",
                features,
                &BuildMode::Release,
                network,
                verbosity,
                unstable_flags,
                target,
            )?;
            (None, None, None)
        }
        BuildArtifacts::CodeOnly => {
            // when building only the code metadata will become stale
            clean_metadata();
            let (opt_result, _, dest_wasm, _) =
                local_build(&crate_metadata, &optimization_passes, &args)?;
            (opt_result, None, Some(dest_wasm))
        }
        BuildArtifacts::All => {
            let (opt_result, build_info, dest_wasm, build_steps) =
                local_build(&crate_metadata, &optimization_passes, &args).map_err(
                    |e| {
                        // build error -> bundle is stale
                        clean_metadata();
                        e
                    },
                )?;

            let metadata_result = MetadataArtifacts {
                dest_metadata: crate_metadata.metadata_path(),
                dest_bundle: crate_metadata.contract_bundle_path(),
            };

            // skip metadata generation if contract unchanged and all metadata artifacts
            // exist.
            if opt_result.is_some()
                || !metadata_result.dest_metadata.exists()
                || !metadata_result.dest_bundle.exists()
            {
                // if metadata build fails after a code build it might become stale
                clean_metadata();
                metadata::execute(
                    &crate_metadata,
                    dest_wasm.as_path(),
                    &metadata_result,
                    features,
                    *network,
                    *verbosity,
                    build_steps,
                    unstable_flags,
                    build_info,
                )?;
            }
            (opt_result, Some(metadata_result), Some(dest_wasm))
        }
    };

    Ok(BuildResult {
        dest_wasm,
        metadata_result,
        target_directory: crate_metadata.target_directory,
        optimization_result: opt_result,
        build_mode: *build_mode,
        build_artifact: *build_artifact,
        verbosity: *verbosity,
        image: None,
        output_type: output_type.clone(),
    })
}

fn local_build(
    crate_metadata: &CrateMetadata,
    optimization_passes: &OptimizationPasses,
    args: &ExecuteArgs,
) -> Result<(Option<OptimizationResult>, BuildInfo, PathBuf, BuildSteps)> {
    let ExecuteArgs {
        verbosity,
        features,
        build_mode,
        network,
        build_artifact,
        unstable_flags,
        keep_debug_symbols,
        lint,
        skip_wasm_validation,
        target,
        max_memory_pages,
        ..
    } = args;

    let mut build_steps = BuildSteps::new();
    let pre_fingerprint = Fingerprint::new(crate_metadata)?;

    maybe_println!(
        verbosity,
        " {} {}",
        format!("{build_steps}").bold(),
        "Building cargo project".bright_green().bold()
    );
    build_steps.increment_current();
    exec_cargo_for_onchain_target(
        crate_metadata,
        "build",
        features,
        build_mode,
        network,
        verbosity,
        unstable_flags,
        target,
    )?;

    // we persist the latest target we used so we trigger a rebuild when we switch
    fs::write(&crate_metadata.target_file_path, target.llvm_target())?;

    let cargo_contract_version = if let Ok(version) = Version::parse(VERSION) {
        version
    } else {
        anyhow::bail!(
            "Unable to parse version number for the currently running \
                    `cargo-contract` binary."
        );
    };

    let build_info = BuildInfo {
        rust_toolchain: util::rust_toolchain()?,
        cargo_contract_version,
        build_mode: *build_mode,
        wasm_opt_settings: WasmOptSettings {
            optimization_passes: *optimization_passes,
            keep_debug_symbols: *keep_debug_symbols,
        },
    };

    let post_fingerprint = Fingerprint::new(crate_metadata)?.ok_or_else(|| {
        anyhow::anyhow!(
            "Expected '{}' to be generated by build",
            crate_metadata.original_code.display()
        )
    })?;

    tracing::debug!(
        "Fingerprint before build: {:?}, after build: {:?}",
        pre_fingerprint,
        post_fingerprint
    );

    let dest_code_path = crate_metadata.dest_code.clone();

    if pre_fingerprint == Some(post_fingerprint) && crate_metadata.dest_code.exists() {
        tracing::info!(
            "No changes in the original wasm at {}, fingerprint {:?}. \
                Skipping Wasm optimization and metadata generation.",
            crate_metadata.original_code.display(),
            pre_fingerprint
        );
        return Ok((None, build_info, dest_code_path, build_steps))
    }

    maybe_lint(
        &mut build_steps,
        *build_artifact,
        *lint,
        crate_metadata,
        verbosity,
    )?;

    maybe_println!(
        verbosity,
        " {} {}",
        format!("{build_steps}").bold(),
        "Post processing code".bright_green().bold()
    );
    build_steps.increment_current();

    // remove build artifacts so we don't have anything stale lingering around
    for t in Target::iter() {
        fs::remove_file(crate_metadata.dest_code.with_extension(t.dest_extension())).ok();
    }

    let original_size =
        fs::metadata(&crate_metadata.original_code)?.len() as f64 / 1000.0;

    match target {
        Target::Wasm => {
            post_process_wasm(
                crate_metadata,
                *skip_wasm_validation,
                verbosity,
                *max_memory_pages,
            )?;
            let handler = WasmOptHandler::new(*optimization_passes, *keep_debug_symbols)?;
            handler.optimize(
                &crate_metadata.dest_code,
                &crate_metadata.contract_artifact_name,
            )?;
        }
        Target::RiscV => {
            fs::copy(&crate_metadata.original_code, &crate_metadata.dest_code)?;
        }
    }

    let optimized_size = fs::metadata(&dest_code_path)?.len() as f64 / 1000.0;

    let optimization_result = OptimizationResult {
        original_size,
        optimized_size,
    };

    Ok((
        Some(optimization_result),
        build_info,
        crate_metadata.dest_code.clone(),
        build_steps,
    ))
}

pub fn maybe_lint(
    steps: &mut BuildSteps,
    build_artifact: BuildArtifacts,
    lint: bool,
    crate_metadata: &CrateMetadata,
    verbosity: &Verbosity,
) -> Result<()> {
    let total_steps = build_artifact.steps();
    if lint {
        steps.set_total_steps(total_steps + 1);
        maybe_println!(
            verbosity,
            " {} {}",
            format!("{steps}").bold(),
            "Checking ink! linting rules".bright_green().bold()
        );
        steps.increment_current();
        exec_cargo_dylint(crate_metadata, *verbosity)?;
        Ok(())
    } else {
        steps.set_total_steps(total_steps);
        Ok(())
    }
}

/// Unique fingerprint for a file to detect whether it has changed.
#[derive(Debug, Eq, PartialEq)]
struct Fingerprint {
    path: PathBuf,
    hash: [u8; 32],
    modified: std::time::SystemTime,
    target: String,
}

impl Fingerprint {
    fn new(crate_metadata: &CrateMetadata) -> Result<Option<Fingerprint>> {
        let code_path = &crate_metadata.original_code;
        let target_path = &crate_metadata.target_file_path;
        if code_path.exists() {
            let modified = fs::metadata(code_path)?.modified()?;
            let bytes = fs::read(code_path)?;
            let hash = blake2_hash(&bytes);
            Ok(Some(Self {
                path: code_path.clone(),
                hash,
                modified,
                target: fs::read_to_string(target_path).with_context(|| {
                    format!(
                        "Cannot read {}.\n A clean build will fix this.",
                        target_path.display()
                    )
                })?,
            }))
        } else {
            Ok(None)
        }
    }
}

/// Returns the blake2 hash of the code slice.
pub fn code_hash(code: &[u8]) -> [u8; 32] {
    blake2_hash(code)
}

/// Returns the blake2 hash of the given bytes.
fn blake2_hash(code: &[u8]) -> [u8; 32] {
    use blake2::digest::{
        consts::U32,
        Digest as _,
    };
    let mut blake2 = blake2::Blake2b::<U32>::new();
    blake2.update(code);
    let result = blake2.finalize();
    result.into()
}

/// Testing individual functions where the build itself is not actually invoked. See
/// [`tests`] for all tests which invoke the `build` command.
#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::{
        util::tests::{
            with_new_contract_project,
            TestContractManifest,
        },
        Verbosity,
    };
    use semver::Version;

    #[test]
    pub fn debug_mode_must_be_compatible() {
        assert_debug_mode_supported(
            &Version::parse("3.0.0-rc4").expect("parsing must work"),
        )
        .expect("debug mode must be compatible");
        assert_debug_mode_supported(
            &Version::parse("4.0.0-rc1").expect("parsing must work"),
        )
        .expect("debug mode must be compatible");
        assert_debug_mode_supported(&Version::parse("5.0.0").expect("parsing must work"))
            .expect("debug mode must be compatible");
    }

    #[test]
    pub fn debug_mode_must_be_incompatible() {
        let res = assert_debug_mode_supported(
            &Version::parse("3.0.0-rc3").expect("parsing must work"),
        )
        .expect_err("assertion must fail");
        assert_eq!(
            res.to_string(),
            "Building the contract in debug mode requires an ink! version newer than `3.0.0-rc3`!"
        );
    }

    #[test]
    fn project_template_dependencies_must_be_ink_compatible() {
        with_new_contract_project(|manifest_path| {
            // given
            // the manifest path

            // when
            let res =
                assert_compatible_ink_dependencies(&manifest_path, Verbosity::Default);

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
            let mut manifest = TestContractManifest::new(manifest_path.clone())?;
            manifest.set_dependency_version("scale", "1.0.0")?;
            manifest.write()?;

            // when
            let res =
                assert_compatible_ink_dependencies(&manifest_path, Verbosity::Default);

            // then
            assert!(res.is_err());
            Ok(())
        })
    }

    #[test]
    fn build_result_seralization_sanity_check() {
        // given
        let raw_result = r#"{
  "dest_wasm": "/path/to/contract.wasm",
  "metadata_result": {
    "dest_metadata": "/path/to/contract.json",
    "dest_bundle": "/path/to/contract.contract"
  },
  "target_directory": "/path/to/target",
  "optimization_result": {
    "dest_wasm": "/path/to/contract.wasm",
    "original_size": 64.0,
    "optimized_size": 32.0
  },
  "build_mode": "Debug",
  "build_artifact": "All",
  "verbosity": "Quiet"
}"#;

        let build_result = BuildResult {
            dest_wasm: Some(PathBuf::from("/path/to/contract.wasm")),
            metadata_result: Some(MetadataArtifacts {
                dest_metadata: PathBuf::from("/path/to/contract.json"),
                dest_bundle: PathBuf::from("/path/to/contract.contract"),
            }),
            target_directory: PathBuf::from("/path/to/target"),
            optimization_result: Some(OptimizationResult {
                original_size: 64.0,
                optimized_size: 32.0,
            }),
            build_mode: Default::default(),
            build_artifact: Default::default(),
            image: None,
            verbosity: Verbosity::Quiet,
            output_type: OutputType::Json,
        };

        // when
        let serialized_result = build_result.serialize_json();

        // then
        assert!(serialized_result.is_ok());
        assert_eq!(serialized_result.unwrap(), raw_result);
    }
}

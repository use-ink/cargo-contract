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

#[cfg(test)]
mod tests;

use crate::{
    crate_metadata::{
        get_cargo_workspace_members,
        CrateMetadata,
    },
    maybe_println,
    util,
    validate_wasm,
    wasm_opt::WasmOptHandler,
    workspace::{
        Manifest,
        ManifestPath,
        Profile,
        Workspace,
    },
    BuildArtifacts,
    BuildMode,
    BuildResult,
    Network,
    OptimizationPasses,
    OptimizationResult,
    OutputType,
    UnstableFlags,
    UnstableOptions,
    Verbosity,
    VerbosityFlags,
};
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
    convert::TryFrom,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
    str,
};

/// This is the maximum number of pages available for a contract to allocate.
const MAX_MEMORY_PAGES: u32 = 16;

/// Arguments to use when executing `build` or `check` commands.
#[derive(Default, Clone)]
pub(crate) struct ExecuteArgs {
    /// The location of the Cargo manifest (`Cargo.toml`) file to use.
    pub manifest_path: ManifestPath,
    pub verbosity: Verbosity,
    pub build_mode: BuildMode,
    pub network: Network,
    pub build_artifact: BuildArtifacts,
    pub unstable_flags: UnstableFlags,
    pub optimization_passes: OptimizationPasses,
    pub keep_debug_symbols: bool,
    pub skip_linting: bool,
    pub output_type: OutputType,
    pub counter: Option<(usize, usize)>,
}

/// Executes build of the smart contract which produces a Wasm binary that is ready for deploying.
///
/// It does so by invoking `cargo build` and then post processing the final binary.
#[derive(Debug, Default, clap::Args)]
#[clap(name = "build")]
pub struct BuildCommand {
    /// Contract package to build
    #[clap(long, short)]
    package: Option<String>,
    /// Path to the `Cargo.toml` of the contract to build
    #[clap(long, parse(from_os_str))]
    manifest_path: Option<PathBuf>,
    /// By default the contract is compiled with debug functionality
    /// included. This enables the contract to output debug messages,
    /// but increases the contract size and the amount of gas used.
    ///
    /// A production contract should always be build in `release` mode!
    /// Then no debug functionality is compiled into the contract.
    #[clap(long = "--release")]
    build_release: bool,
    /// Build all contract packages in the workspace
    #[clap(long = "--all")]
    build_all: bool,
    /// Build offline
    #[clap(long = "--offline")]
    build_offline: bool,
    /// Skips linting checks during the build process
    #[clap(long = "--skip-linting")]
    skip_linting: bool,
    /// Which build artifacts to generate.
    ///
    /// - `all`: Generate the Wasm, the metadata and a bundled `<name>.contract` file.
    ///
    /// - `code-only`: Only the Wasm is created, generation of metadata and a bundled
    ///   `<name>.contract` file is skipped.
    ///
    /// - `check-only`: No artifacts produced: runs the `cargo check` command for the Wasm target,
    ///    only checks for compilation errors.
    #[clap(long = "generate", arg_enum, default_value = "all")]
    build_artifact: BuildArtifacts,
    #[clap(flatten)]
    verbosity: VerbosityFlags,
    #[clap(flatten)]
    unstable_options: UnstableOptions,
    /// Number of optimization passes, passed as an argument to `wasm-opt`.
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
    #[clap(long)]
    optimization_passes: Option<OptimizationPasses>,
    /// Do not remove symbols (Wasm name section) when optimizing.
    ///
    /// This is useful if one wants to analyze or debug the optimized binary.
    #[clap(long)]
    keep_debug_symbols: bool,

    /// Export the build output in JSON format.
    #[clap(long, conflicts_with = "verbose")]
    output_json: bool,
}

impl BuildCommand {
    pub fn exec(&self) -> Result<Vec<BuildResult>> {
        let manifest_path = match self.package.as_ref() {
            Some(package) => {
                let root_manifest_path =
                    ManifestPath::try_from(self.manifest_path.as_ref())?;
                root_manifest_path
                    .subcontract_manifest_path(package)
                    .context(format!(
                        "error: package ID specification `{}` did not match any packages",
                        package
                    ))?
            }
            None => ManifestPath::try_from(self.manifest_path.as_ref())?,
        };
        let unstable_flags: UnstableFlags =
            TryFrom::<&UnstableOptions>::try_from(&self.unstable_options)?;
        let mut verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;

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

        let network = match self.build_offline {
            true => Network::Offline,
            false => Network::Online,
        };

        let output_type = match self.output_json {
            true => OutputType::Json,
            false => OutputType::HumanReadable,
        };

        // We want to ensure that the only thing in `STDOUT` is our JSON formatted string.
        if matches!(output_type, OutputType::Json) {
            verbosity = Verbosity::Quiet;
        }

        let mut build_results = Vec::new();

        let mut args = ExecuteArgs {
            manifest_path: manifest_path.clone(),
            verbosity,
            build_mode,
            network,
            build_artifact: self.build_artifact,
            unstable_flags,
            optimization_passes,
            keep_debug_symbols: self.keep_debug_symbols,
            skip_linting: self.skip_linting,
            output_type,
            counter: None,
        };

        match self.build_all {
            true => {
                let workspace_members = get_cargo_workspace_members(&manifest_path)?;
                for (i, package_id) in workspace_members.iter().enumerate() {
                    // override args for each workspace member
                    args.manifest_path =
                        util::extract_subcontract_manifest_path(package_id.clone())
                            .expect("Error extracting package manifest path");
                    args.counter = Some((i + 1, workspace_members.len()));
                    build_results.push(execute(args.clone())?);
                }
            }
            false => {
                build_results.push(execute(args)?);
            }
        }

        Ok(build_results)
    }
}

#[derive(Debug, clap::Args)]
#[clap(name = "check")]
pub struct CheckCommand {
    /// Contract package to check.
    #[clap(short, long)]
    package: Option<String>,
    /// Path to the `Cargo.toml` of the contract to build
    #[clap(long, parse(from_os_str))]
    manifest_path: Option<PathBuf>,
    /// Check all contract packages in the workspace
    #[clap(long = "--all")]
    check_all: bool,
    #[clap(flatten)]
    verbosity: VerbosityFlags,
    #[clap(flatten)]
    unstable_options: UnstableOptions,
}

impl CheckCommand {
    pub fn exec(&self) -> Result<Vec<BuildResult>> {
        let manifest_path = match self.package.as_ref() {
            Some(package) => {
                let root_manifest_path =
                    ManifestPath::try_from(self.manifest_path.as_ref())?;
                root_manifest_path
                    .subcontract_manifest_path(package)
                    .context(format!(
                        "error: package ID specification `{}` did not match any packages",
                        package
                    ))?
            }
            None => ManifestPath::try_from(self.manifest_path.as_ref())?,
        };
        let unstable_flags: UnstableFlags =
            TryFrom::<&UnstableOptions>::try_from(&self.unstable_options)?;
        let verbosity: Verbosity = TryFrom::<&VerbosityFlags>::try_from(&self.verbosity)?;

        let mut check_results = Vec::new();

        let mut args = ExecuteArgs {
            manifest_path: manifest_path.clone(),
            verbosity,
            build_mode: BuildMode::Debug,
            network: Network::default(),
            build_artifact: BuildArtifacts::CheckOnly,
            unstable_flags,
            optimization_passes: OptimizationPasses::Zero,
            keep_debug_symbols: false,
            skip_linting: false,
            output_type: OutputType::default(),
            counter: None,
        };

        match self.check_all {
            true => {
                let workspace_members = get_cargo_workspace_members(&manifest_path)?;
                for (i, package_id) in workspace_members.iter().enumerate() {
                    // override args for each workspace member
                    args.manifest_path =
                        util::extract_subcontract_manifest_path(package_id.clone())
                            .expect("Error extracting package manifest path");
                    args.counter = Some((i + 1, workspace_members.len()));

                    check_results.push(execute(args.clone())?);
                }
            }
            false => {
                check_results.push(execute(args)?);
            }
        }
        Ok(check_results)
    }
}

/// Executes the supplied cargo command on the project in the specified directory, defaults to the
/// current directory.
///
/// Uses the unstable cargo feature [`build-std`](https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#build-std)
/// to build the standard library with [`panic_immediate_abort`](https://github.com/johnthagen/min-sized-rust#remove-panic-string-formatting-with-panic_immediate_abort)
/// which reduces the size of the Wasm binary by not including panic strings and formatting code.
///
/// # `Cargo.toml` optimizations
///
/// The original `Cargo.toml` will be amended to remove the `rlib` crate type in order to minimize
/// the final Wasm binary size.
///
/// Preferred default `[profile.release]` settings will be added if they are missing, existing
/// user-defined settings will be preserved.
///
/// The `[workspace]` will be added if it is missing to ignore `workspace` from parent `Cargo.toml`.
///
/// To disable this and use the original `Cargo.toml` as is then pass the `-Z original_manifest` flag.
fn exec_cargo_for_wasm_target(
    crate_metadata: &CrateMetadata,
    command: &str,
    build_mode: BuildMode,
    network: Network,
    verbosity: Verbosity,
    unstable_flags: &UnstableFlags,
) -> Result<()> {
    let cargo_build = |manifest_path: &ManifestPath| {
        let target_dir = &crate_metadata.target_directory;
        let target_dir = format!("--target-dir={}", target_dir.to_string_lossy());
        let mut args = vec![
            "--target=wasm32-unknown-unknown",
            "-Zbuild-std",
            "--no-default-features",
            "--release",
            &target_dir,
        ];
        if network == Network::Offline {
            args.push("--offline");
        }
        if build_mode == BuildMode::Debug {
            args.push("--features=ink_env/ink-debug");
        } else {
            args.push("-Zbuild-std-features=panic_immediate_abort");
        }
        let env = vec![(
            "RUSTFLAGS",
            Some("-C link-arg=-zstack-size=65536 -C link-arg=--import-memory -Clinker-plugin-lto"),
        )];
        util::invoke_cargo(command, &args, manifest_path.directory(), verbosity, env)?;

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
                    .with_profile_release_defaults(Profile::default_contract_release())?
                    .with_workspace()?;
                Ok(())
            })?
            .using_temp(cargo_build)?;
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
        // This is because we build from a temporary directory (to patch the manifest) but still
        // want the output to live at a fixed path. `cargo dylint` does not accept this information
        // on the command line.
        ("CARGO_TARGET_DIR", Some(target_dir.as_ref())),
        // There are generally problems with having a custom `rustc` wrapper, while
        // executing `dylint` (which has a custom linker). Especially for `sccache`
        // there is this bug: https://github.com/mozilla/sccache/issues/1000.
        // Until we have a justification for leaving the wrapper we should unset it.
        ("RUSTC_WRAPPER", None),
    ];

    Workspace::new(&crate_metadata.cargo_meta, &crate_metadata.root_package.id)?
        .with_root_package_manifest(|manifest| {
            manifest.with_dylint()?;
            Ok(())
        })?
        .using_temp(|manifest_path| {
            util::invoke_cargo("dylint", &args, manifest_path.directory(), verbosity, env)
                .map(|_| ())
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
    // which just ouputs some information. It always needs to do some linking in
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
/// Iterates over the import section, finds the memory import entry if any and adjusts the maximum
/// limit.
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
        *mem_ty = MemoryType::new(initial, Some(MAX_MEMORY_PAGES));
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
/// Any elements not referenced by these exports become orphaned and are removed by `wasm-opt`.
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
fn post_process_wasm(crate_metadata: &CrateMetadata) -> Result<()> {
    // Deserialize Wasm module from a file.
    let mut module = load_module(&crate_metadata.original_wasm)
        .context("Loading of original wasm failed")?;

    strip_exports(&mut module);
    ensure_maximum_memory_pages(&mut module, MAX_MEMORY_PAGES)?;
    strip_custom_sections(&mut module);

    validate_wasm::validate_import_section(&module)?;

    debug_assert!(
        !module.clone().into_bytes().unwrap().is_empty(),
        "resulting wasm size of post processing must be > 0"
    );

    parity_wasm::serialize_to_file(&crate_metadata.dest_wasm, module)?;
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
        let _ = util::invoke_cargo("tree", &args, manifest_path.directory(), verbosity, vec![])
            .with_context(|| {
                format!(
                    "Mismatching versions of `{}` were found!\n\
                     Please ensure that your contract and your ink! dependencies use a compatible \
                     version of this package.",
                    dependency
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

/// Executes build of the smart contract which produces a Wasm binary that is ready for deploying.
///
/// It does so by invoking `cargo build` and then post processing the final binary.
pub(crate) fn execute(args: ExecuteArgs) -> Result<BuildResult> {
    let ExecuteArgs {
        manifest_path,
        verbosity,
        build_mode,
        network,
        build_artifact,
        unstable_flags,
        optimization_passes,
        keep_debug_symbols,
        skip_linting,
        output_type,
        counter,
    } = args;

    let crate_metadata = CrateMetadata::collect(&manifest_path)?;

    assert_compatible_ink_dependencies(&manifest_path, verbosity)?;
    if build_mode == BuildMode::Debug {
        assert_debug_mode_supported(&crate_metadata.ink_version)?;
    }

    let build = || -> Result<OptimizationResult> {
        if let Some((x, y)) = counter {
            maybe_println!(
                verbosity,
                "\n {} {} {}",
                "Building contract:".bright_purple().bold(),
                crate_metadata.contract_artifact_name,
                format!("[{}/{}]", x, y).bold(),
            );
        }
        if skip_linting {
            maybe_println!(
                verbosity,
                " {} {}",
                format!("[1/{}]", build_artifact.steps()).bold(),
                "Skip ink! linting rules".bright_yellow().bold()
            );
        } else {
            maybe_println!(
                verbosity,
                " {} {}",
                format!("[1/{}]", build_artifact.steps()).bold(),
                "Checking ink! linting rules".bright_green().bold()
            );
            exec_cargo_dylint(&crate_metadata, verbosity)?;
        }

        maybe_println!(
            verbosity,
            " {} {}",
            format!("[2/{}]", build_artifact.steps()).bold(),
            "Building cargo project".bright_green().bold()
        );
        exec_cargo_for_wasm_target(
            &crate_metadata,
            "build",
            build_mode,
            network,
            verbosity,
            &unstable_flags,
        )?;

        maybe_println!(
            verbosity,
            " {} {}",
            format!("[3/{}]", build_artifact.steps()).bold(),
            "Post processing wasm file".bright_green().bold()
        );
        post_process_wasm(&crate_metadata)?;

        maybe_println!(
            verbosity,
            " {} {}",
            format!("[4/{}]", build_artifact.steps()).bold(),
            "Optimizing wasm file".bright_green().bold()
        );

        let handler = WasmOptHandler::new(optimization_passes, keep_debug_symbols)?;
        let optimization_result = handler.optimize(
            &crate_metadata.dest_wasm,
            &crate_metadata.contract_artifact_name,
        )?;

        Ok(optimization_result)
    };

    let (opt_result, metadata_result) = match build_artifact {
        BuildArtifacts::CheckOnly => {
            if skip_linting {
                maybe_println!(
                    verbosity,
                    " {} {}",
                    format!("[1/{}]", build_artifact.steps()).bold(),
                    "Skip ink! linting rules".bright_yellow().bold()
                );
            } else {
                maybe_println!(
                    verbosity,
                    " {} {}",
                    format!("[1/{}]", build_artifact.steps()).bold(),
                    "Checking ink! linting rules".bright_green().bold()
                );
                exec_cargo_dylint(&crate_metadata, verbosity)?;
            }

            if let Some((x, y)) = counter {
                maybe_println!(
                    verbosity,
                    "\n {} {} {}",
                    "Checking contract:".bright_purple().bold(),
                    crate_metadata.contract_artifact_name,
                    format!("[{}/{}]", x, y).bold(),
                );
            }

            maybe_println!(
                verbosity,
                " {} {}",
                format!("[2/{}]", build_artifact.steps()).bold(),
                "Executing `cargo check`".bright_green().bold()
            );
            let res = exec_cargo_for_wasm_target(
                &crate_metadata,
                "check",
                BuildMode::Release,
                network,
                verbosity,
                &unstable_flags,
            );

            if res.is_ok() {
                maybe_println!(
                    verbosity,
                    " {} {} {}",
                    "Contract".bright_purple().bold(),
                    crate_metadata.contract_artifact_name,
                    "was checked successfully.\n".bright_purple().bold()
                );
            }

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
                network,
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
        output_type,
    })
}

/// Testing individual functions where the build itself is not actually invoked. See [`tests`] for
/// all tests which invoke the `build` command.
#[cfg(test)]
mod unit_tests {
    use super::{
        assert_compatible_ink_dependencies,
        assert_debug_mode_supported,
    };
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
}

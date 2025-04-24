// Copyright (C) Use Ink (UK) Ltd.
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

use contract_metadata::{
    compatibility::check_contract_ink_compatibility,
    ContractMetadata,
};
pub use lint::lint;
use which as _;

mod args;
mod crate_metadata;
mod docker;
mod lint;
pub mod metadata;
mod new;
mod solidity_metadata;
#[cfg(test)]
mod tests;
pub mod util;
mod validate_bytecode;
mod workspace;

#[deprecated(since = "2.0.2", note = "Use MetadataArtifacts instead")]
pub use self::metadata::InkMetadataArtifacts as MetadataResult;

pub use self::{
    args::{
        BuildArtifacts,
        BuildMode,
        Features,
        MetadataSpec,
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
        InkMetadataArtifacts,
        MetadataArtifacts,
    },
    new::new_contract_project,
    solidity_metadata::SolidityMetadataArtifacts,
    util::DEFAULT_KEY_COL_WIDTH,
    workspace::{
        Lto,
        Manifest,
        ManifestPath,
        OptLevel,
        PanicStrategy,
        Profile,
        Workspace,
    },
};

pub use docker::{
    docker_build,
    ImageVariant,
};

use anyhow::{
    bail,
    Context,
    Result,
};
use colored::Colorize;
use regex::Regex;
use semver::Version;
use std::{
    cmp::PartialEq,
    fs,
    path::PathBuf,
    str,
};

/// Version of the currently executing `cargo-contract` binary.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Result of linking an ELF woth PolkaVM.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct LinkerSizeResult {
    /// The original ELF size.
    pub original_size: f64,
    /// The size after linking with PolkaVM.
    pub optimized_size: f64,
}

/// Arguments to use when executing `build` or `check` commands.
#[derive(Default, Clone)]
pub struct ExecuteArgs {
    /// The location of the Cargo manifest (`Cargo.toml`) file to use.
    pub manifest_path: ManifestPath,
    pub verbosity: Verbosity,
    pub build_mode: BuildMode,
    pub features: Features,
    pub network: Network,
    pub build_artifact: BuildArtifacts,
    pub unstable_flags: UnstableFlags,
    pub keep_debug_symbols: bool,
    pub extra_lints: bool,
    pub output_type: OutputType,
    pub image: ImageVariant,
    pub metadata_spec: MetadataSpec,
}

/// Result of the build process.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct BuildResult {
    /// Path to the resulting binary file.
    pub dest_binary: Option<PathBuf>,
    /// Result of the metadata generation.
    pub metadata_result: Option<MetadataArtifacts>,
    /// Path to the directory where output files are written to.
    pub target_directory: PathBuf,
    /// If existent the result of the linking.
    pub linker_size_result: Option<LinkerSizeResult>,
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
        let opt_size_diff = if let Some(ref opt_result) = self.linker_size_result {
            let size_diff = format!(
                "\nOriginal size: {}, Optimized: {}\n\n",
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
                self.dest_binary
                    .as_ref()
                    .expect("polkavm path must exist")
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
            let (dest, desc) = match metadata_result {
                MetadataArtifacts::Ink(ink_metadata_artifacts) => {
                    (&ink_metadata_artifacts.dest_bundle, "code + metadata")
                }
                MetadataArtifacts::Solidity(solidity_metadata_artifacts) => {
                    (
                        &solidity_metadata_artifacts.dest_metadata,
                        "Solidity compatible metadata",
                    )
                }
            };
            let bundle = format!("  - {} ({})\n", util::base_name(dest).bold(), desc);
            out.push_str(&bundle);
        }
        if let Some(dest_binary) = self.dest_binary.as_ref() {
            let path = format!(
                "  - {} (the contract's code)\n",
                util::base_name(dest_binary).bold()
            );
            out.push_str(&path);
        }
        if let Some(metadata_result) = self.metadata_result.as_ref() {
            let (dest, desc) = match metadata_result {
                MetadataArtifacts::Ink(ink_metadata_artifacts) => {
                    (&ink_metadata_artifacts.dest_metadata, "metadata")
                }
                MetadataArtifacts::Solidity(solidity_metadata_artifacts) => {
                    (
                        &solidity_metadata_artifacts.dest_abi,
                        "Solidity compatible ABI",
                    )
                }
            };
            let metadata = format!(
                "  - {} (the contract's {})",
                util::base_name(dest).bold(),
                desc
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
/// which reduces the size of the contract binary by not including panic strings and
/// formatting code.
///
/// # `Cargo.toml` optimizations
///
/// The original `Cargo.toml` will be amended to remove the `rlib` crate type in order to
/// minimize the final contract binary size.
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
) -> Result<()> {
    let cargo_build = |manifest_path: &ManifestPath| {
        let target_dir = format!(
            "--target-dir={}",
            crate_metadata.target_directory.to_string_lossy()
        );

        let mut args = vec![
            format!("--target={}", Target::llvm_target(crate_metadata)),
            "--release".to_owned(),
            target_dir,
        ];
        args.extend(onchain_cargo_options(crate_metadata));
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

        // merge target specific flags with the common flags (defined here)
        // We want to disable warnings here as they will be duplicates of the clippy pass.
        // However, if we want to do so with either `--cap-lints allow` or  `-A
        // warnings` the build will fail. It seems that the cross compilation
        // depends on some warning to be enabled. Until we figure that out we need
        // to live with duplicated warnings. For the metadata build we can disable
        // warnings.
        let rustflags = {
            let common_flags = "-Clinker-plugin-lto\x1f-Clink-arg=-zstack-size=4096";
            if let Some(target_flags) = Target::rustflags() {
                format!("{}\x1f{}", common_flags, target_flags)
            } else {
                common_flags.to_string()
            }
        };

        fs::create_dir_all(&crate_metadata.target_directory)?;
        env.push(("CARGO_ENCODED_RUSTFLAGS", Some(rustflags)));

        execute_cargo(util::cargo_cmd(
            command,
            &args,
            manifest_path.directory(),
            *verbosity,
            env,
        ))
    };

    if unstable_flags.original_manifest {
        verbose_eprintln!(
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
                    .with_merged_workspace_dependencies(crate_metadata)?
                    .with_empty_workspace();
                Ok(())
            })?
            .using_temp(cargo_build)?;
    }

    Ok(())
}

/// Check if the `INK_STATIC_BUFFER_SIZE` is set.
/// If so, then checks if the current contract has already been compiled with a new value.
/// If not, or metadata is not present, we need to clean binaries and rebuild.
fn check_buffer_size_invoke_cargo_clean(
    crate_metadata: &CrateMetadata,
    verbosity: &Verbosity,
) -> Result<()> {
    if let Ok(buffer_size) = std::env::var("INK_STATIC_BUFFER_SIZE") {
        let buffer_size_value: u64 = buffer_size
            .parse()
            .context("`INK_STATIC_BUFFER_SIZE` must have an integer value.")?;

        let extract_buffer_size = |metadata_path: PathBuf| -> Result<u64> {
            let size = ContractMetadata::load(metadata_path)
                .context("Metadata is not present")?
                .abi
                // get `spec` field
                .get("spec")
                .context("spec field should be present in ABI.")?
                // get `environment` field
                .get("environment")
                .context("environment field should be present in ABI.")?
                // get `staticBufferSize` field
                .get("staticBufferSize")
                .context("`staticBufferSize` must be specified.")?
                // convert to u64
                .as_u64()
                .context("`staticBufferSize` value must be an integer.")?;

            Ok(size)
        };

        let cargo = util::cargo_cmd(
            "clean",
            Vec::<&str>::new(),
            crate_metadata.manifest_path.directory(),
            *verbosity,
            vec![],
        );

        match extract_buffer_size(crate_metadata.metadata_path()) {
            Ok(contract_buffer_size) if contract_buffer_size == buffer_size_value => {
                verbose_eprintln!(
                    verbosity,
                    "{} {}",
                    "info:".green().bold(),
                    "Detected a configured buffer size, but the value is already specified."
                        .bold()
                );
            }
            Ok(_) => {
                verbose_eprintln!(
                    verbosity,
                    "{} {}",
                    "warning:".yellow().bold(),
                    "Detected a change in the configured buffer size. Rebuilding the project."
                        .bold()
                );
                execute_cargo(cargo)?;
            }
            Err(_) => {
                verbose_eprintln!(
                    verbosity,
                    "{} {}",
                    "warning:".yellow().bold(),
                    "Cannot find the previous size of the static buffer. Rebuilding the project."
                        .bold()
                );
                execute_cargo(cargo)?;
            }
        }
    }
    Ok(())
}

/// Executes the supplied cargo command, reading the output and scanning for known errors.
/// Writes the captured stderr back to stderr and maintains the cargo tty progress bar.
fn execute_cargo(cargo: duct::Expression) -> Result<()> {
    match cargo.unchecked().run() {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => anyhow::bail!(String::from_utf8_lossy(&out.stderr).to_string()),
        Err(e) => anyhow::bail!("Cannot run `cargo` command: {:?}", e),
    }
}

/// Returns a list of cargo options used for on-chain builds
fn onchain_cargo_options(crate_metadata: &CrateMetadata) -> Vec<String> {
    vec![
        format!("--target={}", Target::llvm_target(crate_metadata)),
        "-Zbuild-std=core,alloc".to_owned(),
        "--no-default-features".to_owned(),
    ]
}

/// Checks whether the supplied `ink_version` already contains the debug feature.
///
/// This feature was introduced in `3.0.0-rc4` with `ink_env/ink-debug`.
pub fn assert_debug_mode_supported(ink_version: &Version) -> Result<()> {
    tracing::debug!("Contract version: {:?}", ink_version);
    let minimum_version = Version::parse("3.0.0-rc4").expect("parsing version failed");
    if ink_version < &minimum_version {
        anyhow::bail!(
            "Building the contract in debug mode requires an ink! version newer than `3.0.0-rc3`!"
        );
    }
    Ok(())
}

/// Executes build of the smart contract which produces a PolkaVM binary that is ready for
/// deploying.
///
/// It does so by invoking `cargo build` and then post-processing the final binary.
pub fn execute(args: ExecuteArgs) -> Result<BuildResult> {
    let ExecuteArgs {
        manifest_path,
        verbosity,
        features,
        build_mode,
        network,
        build_artifact,
        unstable_flags,
        extra_lints,
        output_type,
        metadata_spec,
        ..
    } = &args;

    // if image exists, then --verifiable was called and we need to build inside docker.
    if build_mode == &BuildMode::Verifiable {
        return docker_build(args)
    }

    let crate_metadata = CrateMetadata::collect(manifest_path)?;

    if build_mode == &BuildMode::Debug {
        assert_debug_mode_supported(&crate_metadata.ink_version)?;
    }

    if let Err(e) = check_contract_ink_compatibility(&crate_metadata.ink_version, None) {
        eprintln!("{} {}", "warning:".yellow().bold(), e.to_string().bold());
    }

    let clean_metadata = || {
        fs::remove_file(crate_metadata.metadata_path()).ok();
        fs::remove_file(crate_metadata.contract_bundle_path()).ok();
        fs::remove_file(solidity_metadata::abi_path(&crate_metadata)).ok();
        fs::remove_file(solidity_metadata::metadata_path(&crate_metadata)).ok();
    };

    let (opt_result, metadata_result, dest_binary) = match build_artifact {
        BuildArtifacts::CheckOnly => {
            // Check basically means only running our linter without building.
            lint(*extra_lints, &crate_metadata, verbosity)?;
            (None, None, None)
        }
        BuildArtifacts::CodeOnly => {
            // when building only the code metadata will become stale
            clean_metadata();
            let (opt_result, _, dest_binary) = local_build(&crate_metadata, &args)?;
            (opt_result, None, Some(dest_binary))
        }
        BuildArtifacts::All => {
            let (opt_result, build_info, dest_binary) =
                local_build(&crate_metadata, &args).inspect_err(|_| {
                    // build error -> bundle is stale
                    clean_metadata();
                })?;

            let metadata_artifacts = match metadata_spec {
                MetadataSpec::Ink => {
                    MetadataArtifacts::Ink(InkMetadataArtifacts {
                        dest_metadata: crate_metadata.metadata_path(),
                        dest_bundle: crate_metadata.contract_bundle_path(),
                    })
                }
                MetadataSpec::Solidity => {
                    MetadataArtifacts::Solidity(SolidityMetadataArtifacts {
                        dest_abi: solidity_metadata::abi_path(&crate_metadata),
                        dest_metadata: solidity_metadata::metadata_path(&crate_metadata),
                    })
                }
            };

            // skip metadata generation if contract is unchanged, metadata spec is
            // unchanged, and all metadata artifacts exist.
            let pre_metadata_spec =
                fs::read_to_string(&crate_metadata.metadata_spec_path);
            let is_unchanged_metadata_spec =
                pre_metadata_spec.ok() == Some(metadata_spec.to_string());
            if opt_result.is_some()
                || !is_unchanged_metadata_spec
                || !metadata_artifacts.exists()
            {
                // Persists the current metadata spec used so we trigger regeneration
                // when we switch
                if !is_unchanged_metadata_spec {
                    fs::write(
                        &crate_metadata.metadata_spec_path,
                        metadata_spec.to_string(),
                    )?;
                }

                // if metadata build fails after a code build it might become stale
                clean_metadata();
                metadata::execute(
                    &crate_metadata,
                    dest_binary.as_path(),
                    &metadata_artifacts,
                    features,
                    *network,
                    *verbosity,
                    unstable_flags,
                    build_info,
                )?;
            }
            (opt_result, Some(metadata_artifacts), Some(dest_binary))
        }
    };

    Ok(BuildResult {
        dest_binary,
        metadata_result,
        target_directory: crate_metadata.target_directory,
        linker_size_result: opt_result,
        build_mode: *build_mode,
        build_artifact: *build_artifact,
        verbosity: *verbosity,
        image: None,
        output_type: output_type.clone(),
    })
}

/// Build the contract on host locally
fn local_build(
    crate_metadata: &CrateMetadata,
    args: &ExecuteArgs,
) -> Result<(Option<LinkerSizeResult>, BuildInfo, PathBuf)> {
    let ExecuteArgs {
        verbosity,
        features,
        build_mode,
        network,
        unstable_flags,
        ..
    } = args;

    let pre_fingerprint = Fingerprint::new(crate_metadata)?;

    verbose_eprintln!(
        verbosity,
        " {} {}",
        "[==]".bold(),
        "Building cargo project".bright_green().bold()
    );
    check_buffer_size_invoke_cargo_clean(crate_metadata, verbosity)?;
    exec_cargo_for_onchain_target(
        crate_metadata,
        "build",
        features,
        build_mode,
        network,
        verbosity,
        unstable_flags,
    )?;

    // We persist the latest target we used so we trigger a rebuild when we switch
    fs::write(
        &crate_metadata.target_file_path,
        Target::llvm_target(crate_metadata),
    )?;

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

    let dest_code_path = crate_metadata.dest_binary.clone();

    if pre_fingerprint == Some(post_fingerprint) && crate_metadata.dest_binary.exists() {
        tracing::info!(
            "No changes in the original PolkaVM binary at {}, fingerprint {:?}. \
                Skipping metadata generation.",
            crate_metadata.original_code.display(),
            pre_fingerprint
        );
        return Ok((None, build_info, dest_code_path))
    }

    verbose_eprintln!(
        verbosity,
        " {} {}",
        "[==]".bold(),
        "Post processing code".bright_green().bold()
    );

    // remove build artifacts so we don't have anything stale lingering around
    fs::remove_file(
        crate_metadata
            .dest_binary
            .with_extension(Target::dest_extension()),
    )
    .ok();

    let original_size =
        fs::metadata(&crate_metadata.original_code)?.len() as f64 / 1000.0;

    let mut config = polkavm_linker::Config::default();
    config.set_strip(true);
    config.set_optimize(true);
    let orig = fs::read(&crate_metadata.original_code)?;

    let linked = match polkavm_linker::program_from_elf(config, orig.as_ref()) {
        Ok(linked) => linked,
        Err(err) => {
            let re =
                Regex::new(r"'(__ink_enforce_error_.*)'").expect("failed creating regex");
            let err = err.to_string();
            let mut ink_err = re.captures_iter(&err).map(|c| c.extract());
            let mut details = String::from("");
            if let Some((_, [ink_err_identifier])) = ink_err.next() {
                details = format!(
                    "\n\n{}",
                    validate_bytecode::parse_linker_error(ink_err_identifier)
                );
            }
            bail!("Failed to link polkavm program: {}{}", err, details)
        }
    };
    fs::write(&crate_metadata.dest_binary, linked)?;

    let optimized_size = fs::metadata(&dest_code_path)?.len() as f64 / 1000.0;

    let optimization_result = LinkerSizeResult {
        original_size,
        optimized_size,
    };

    Ok((
        Some(optimization_result),
        build_info,
        crate_metadata.dest_binary.clone(),
    ))
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

/// Returns the H256 hash of the code slice.
pub fn code_hash(code: &[u8]) -> [u8; 32] {
    h256_hash(code)
}

/// Returns the H256 hash of the given `code` slice.
fn h256_hash(code: &[u8]) -> [u8; 32] {
    use sha3::{
        Digest,
        Keccak256,
    };
    let hash = Keccak256::digest(code);
    let sl = hash.as_slice();
    assert!(sl.len() == 32, "expected length of 32");
    let mut arr = [0u8; 32];
    arr.copy_from_slice(sl);
    arr
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

/// todo
pub fn project_path(path: PathBuf) -> PathBuf {
    if let Ok(cargo_target_dir) = std::env::var("CARGO_TARGET_DIR") {
        PathBuf::from(cargo_target_dir)
    } else {
        path
    }
}

/// Testing individual functions where the build itself is not actually invoked. See
/// [`tests`] for all tests which invoke the `build` command.
#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::Verbosity;
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
    fn build_result_seralization_sanity_check() {
        // given
        let raw_result = r#"{
  "dest_binary": "/path/to/contract.polkavm",
  "metadata_result": {
    "Ink": {
      "dest_metadata": "/path/to/contract.json",
      "dest_bundle": "/path/to/contract.contract"
    }
  },
  "target_directory": "/path/to/target",
  "linker_size_result": {
    "original_size": 64.0,
    "optimized_size": 32.0
  },
  "build_mode": "Debug",
  "build_artifact": "All",
  "verbosity": "Quiet",
  "image": null
}"#;

        let build_result = BuildResult {
            dest_binary: Some(PathBuf::from("/path/to/contract.polkavm")),
            metadata_result: Some(MetadataArtifacts::Ink(InkMetadataArtifacts {
                dest_metadata: PathBuf::from("/path/to/contract.json"),
                dest_bundle: PathBuf::from("/path/to/contract.contract"),
            })),
            target_directory: PathBuf::from("/path/to/target"),
            linker_size_result: Some(LinkerSizeResult {
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

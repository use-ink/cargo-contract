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

use crate::{
    execute_cargo,
    onchain_cargo_options,
    util,
    verbose_eprintln,
    CrateMetadata,
    Verbosity,
    Workspace,
};
use anyhow::Result;
use colored::Colorize;
use std::{
    path::Path,
    process::Command,
};

/// Toolchain used to build ink_linting:
/// https://github.com/use-ink/ink/blob/master/linting/rust-toolchain.toml
pub const TOOLCHAIN_VERSION: &str = "nightly-2025-05-14";
//pub const TOOLCHAIN_VERSION: &str = "nightly-2025-07-16";
/// Git repository with ink_linting libraries
pub const GIT_URL: &str = "https://github.com/use-ink/ink";
// /// Git revision number of the linting crate
pub const GIT_REV: &str = "266ca2f163927fdc3d1f8d994847ea67e4733599";

/// Run linting that involves two steps: `clippy` and `dylint`. Both are mandatory as
/// they're part of the compilation process and implement security-critical features.
pub fn lint(
    extra_lints: bool,
    crate_metadata: &CrateMetadata,
    verbosity: &Verbosity,
) -> Result<()> {
    verbose_eprintln!(
        verbosity,
        " {} {}",
        "[==]".bold(),
        "Checking clippy linting rules".bright_green().bold()
    );
    exec_cargo_clippy(crate_metadata, *verbosity)?;

    // TODO (jubnzv): Dylint needs a custom toolchain installed by the user. Currently,
    // it's required only for RiscV target. We're working on the toolchain integration
    // and will make this step mandatory for all targets in future releases.
    if extra_lints {
        verbose_eprintln!(
            verbosity,
            " {} {}",
            "[==]".bold(),
            "Checking ink! linting rules".bright_green().bold()
        );
        exec_cargo_dylint(extra_lints, crate_metadata, *verbosity)?;
    }

    Ok(())
}

/// Inject our custom lints into the manifest and execute `cargo dylint` .
///
/// We create a temporary folder, extract the linting driver there and run
/// `cargo dylint` with it.
fn exec_cargo_dylint(
    extra_lints: bool,
    crate_metadata: &CrateMetadata,
    verbosity: Verbosity,
) -> Result<()> {
    check_dylint_requirements(crate_metadata.manifest_path.directory())?;

    // `dylint` is verbose by default, it doesn't have a `--verbose` argument,
    let verbosity = match verbosity {
        Verbosity::Verbose => Verbosity::Default,
        Verbosity::Default | Verbosity::Quiet => Verbosity::Quiet,
    };

    let mut args = if extra_lints {
        vec![
            "--lib=ink_linting_mandatory".to_owned(),
            "--lib=ink_linting".to_owned(),
        ]
    } else {
        vec!["--lib=ink_linting_mandatory".to_owned()]
    };
    args.push("--".to_owned());
    // Pass on-chain build options to ensure the linter expands all conditional `cfg_attr`
    // macros, as it does for the release build.
    args.extend(onchain_cargo_options(crate_metadata));

    let target_dir = &crate_metadata.target_directory.to_string_lossy();
    let mut env = vec![
        // We need to set the `CARGO_TARGET_DIR` environment variable in
        // case `cargo dylint` is invoked.
        //
        // This is because we build from a temporary directory (to patch the manifest)
        // but still want the output to live at a fixed path. `cargo dylint` does
        // not accept this information on the command line.
        ("CARGO_TARGET_DIR", Some(target_dir.to_string())),
    ];
    // Substrate has the `cfg` `substrate_runtime` to distinguish if e.g. `sp-io`
    // is being build for `std` or for a Wasm/RISC-V runtime.
    let mut rustflags = "".to_string();

    // Sets ABI `cfg` flags (if necessary).
    if let Some(abi) = crate_metadata.abi {
        rustflags.push(' ');
        rustflags.push_str(&abi.rustflag());

        // Sets a custom `rustc` wrapper which passes compiler flags to `rustc`,
        // because `cargo` doesn't pass compiler flags to proc macros and build
        // scripts when the `--target` flag is set.
        // See `util::rustc_wrapper::env_vars` docs for details.
        if let Some(rustc_wrapper_envs) = util::rustc_wrapper::env_vars(crate_metadata)? {
            env.extend(rustc_wrapper_envs);
        }
    }
    // Sets env vars for passing `rustc` flags.
    env.extend([
        ("DYLINT_RUSTFLAGS", Some(rustflags.clone())),
        ("RUSTFLAGS", Some(rustflags)),
    ]);

    Workspace::new(&crate_metadata.cargo_meta, &crate_metadata.root_package.id)?
        .with_root_package_manifest(|manifest| {
            manifest.with_dylint()?;
            Ok(())
        })?
        .using_temp(|manifest_path| {
            env.extend([(
                "PATH",
                Some(
                    format!(
                        "{}:{}",
                        manifest_path.directory().unwrap().to_string_lossy(),
                        std::env::var("PATH").unwrap()
                    )
                    .to_string(),
                ),
            )]);

            #[cfg(target_os = "windows")]
            panic!("Unfortunately linting is not yet supported under Windows.");

            let cargo_path = which::which("cargo").unwrap();
            let script = r#"
            #!/bin/bash
            if [[ "$1" == "check" ]]; then
                RUSTFLAGS="--cfg=substrate_runtime" cargo_path check "${@:2}"
            else
                cargo_path "$@"
            fi"#;
            let script = script
                .to_string()
                .replace("cargo_path", &cargo_path.to_string_lossy());

            let cargo_wrapper = manifest_path.directory().unwrap().join("cargo");
            std::fs::write(&cargo_wrapper, &script).expect("writing failed");
            make_executable(cargo_wrapper.as_path()).unwrap();

            if let Some(rustc_wrapper_envs) =
                util::rustc_wrapper::env_vars(crate_metadata)?
            {
                env.extend(rustc_wrapper_envs);
            }
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

fn make_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    let mode = permissions.mode();

    // Set executable bit for owner, group, and others (0o111)
    permissions.set_mode(mode | 0o111);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

/// Checks if all requirements for `dylint` are installed.
///
/// We require both `cargo-dylint` and `dylint-link` because the driver is being
/// built at runtime on demand. These must be built using a custom version of the
/// toolchain, as the linter utilizes the unstable rustc API.
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

    // Check if the required toolchain is present and is installed with `rustup`.
    if let Ok(output) = Command::new("rustup").arg("toolchain").arg("list").output() {
        anyhow::ensure!(
            String::from_utf8_lossy(&output.stdout).contains(TOOLCHAIN_VERSION),
            format!(
                "Toolchain `{TOOLCHAIN_VERSION}` was not found!\n\
                This specific version is required to provide additional source code analysis.\n\n\
                You can install it by executing:\n\
                  rustup install {TOOLCHAIN_VERSION}\n\
                  rustup component add rust-src --toolchain {TOOLCHAIN_VERSION}\n\
                  rustup run {TOOLCHAIN_VERSION} cargo install cargo-dylint dylint-link",
            )
            .to_string()
            .bright_yellow());
    } else {
        anyhow::bail!(format!(
            "Toolchain `{TOOLCHAIN_VERSION}` was not found!\n\
            This specific version is required to provide additional source code analysis.\n\n\
            Install `rustup` according to https://rustup.rs/ and then run:\
              rustup install {TOOLCHAIN_VERSION}\n\
              rustup component add rust-src --toolchain {TOOLCHAIN_VERSION}\n\
              rustup run {TOOLCHAIN_VERSION} cargo install cargo-dylint dylint-link",
        )
        .to_string()
        .bright_yellow());
    }

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

/// Run cargo clippy on the unmodified manifest.
fn exec_cargo_clippy(crate_metadata: &CrateMetadata, verbosity: Verbosity) -> Result<()> {
    let args = [
        "--all-features",
        // customize clippy lints after the "--"
        "--",
        // these are hard errors because we want to guarantee that implicit overflows
        // and lossy integer conversions never happen
        // See https://github.com/use-ink/cargo-contract/pull/1190
        "-Dclippy::arithmetic_side_effects",
        // See https://github.com/use-ink/cargo-contract/pull/1895
        // todo remove once the fix for https://github.com/paritytech/parity-scale-codec/issues/713
        // is released.
        // "-Dclippy::cast_possible_truncation",
        "-Dclippy::cast_possible_wrap",
        "-Dclippy::cast_sign_loss",
    ];
    let env = match crate_metadata.abi {
        None => vec![],
        Some(abi) => {
            vec![(
                "CARGO_ENCODED_RUSTFLAGS",
                Some(abi.cargo_encoded_rustflag()),
            )]
        }
    };

    // we execute clippy with the plain manifest no temp dir required
    execute_cargo(util::cargo_cmd(
        "clippy",
        args,
        crate_metadata.manifest_path.directory(),
        verbosity,
        env,
    ))
}

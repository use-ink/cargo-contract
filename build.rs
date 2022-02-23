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

use std::{
    borrow::Cow,
    env,
    ffi::OsStr,
    fs::File,
    io::{prelude::*, Write},
    iter::Iterator,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Result;
use walkdir::WalkDir;
use zip::{write::FileOptions, CompressionMethod, ZipWriter};

use platforms::{TARGET_ARCH, TARGET_ENV, TARGET_OS};
use substrate_build_script_utils::rerun_if_git_head_changed;

const DEFAULT_UNIX_PERMISSIONS: u32 = 0o755;

fn main() {
    generate_cargo_keys();
    rerun_if_git_head_changed();

    let manifest_dir: PathBuf = env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR should be set by cargo")
        .into();
    let out_dir: PathBuf = env::var("OUT_DIR")
        .expect("OUT_DIR should be set by cargo")
        .into();
    let res = zip_template_and_build_dylint_driver(manifest_dir, out_dir);

    match res {
        Ok(()) => std::process::exit(0),
        Err(err) => {
            eprintln!("Encountered error: {:?}", err);
            std::process::exit(1)
        }
    }
}

/// This method:
///   * Creates a zip archive of the `new` project template.
///   * Builds the `dylint` driver found in `ink_linting`, the compiled
///     driver is put into a zip archive as well.
fn zip_template_and_build_dylint_driver(manifest_dir: PathBuf, out_dir: PathBuf) -> Result<()> {
    zip_template(&manifest_dir, &out_dir)?;

    check_dylint_link_installed()?;

    // This zip file will contain the `dylint` driver, this is one file named in the form of
    // `libink_linting@nightly-2021-11-04-x86_64-unknown-linux-gnu.so`. This file is obtained
    // by building the crate in `ink_linting/`.
    let dylint_driver_dst_file = out_dir.join("ink-dylint-driver.zip");
    build_and_zip_dylint_driver(manifest_dir, out_dir, dylint_driver_dst_file)?;

    Ok(())
}

/// Creates a zip archive `template.zip` of the `new` project template in `out_dir`.
fn zip_template(manifest_dir: &Path, out_dir: &Path) -> Result<()> {
    let template_dir = manifest_dir.join("templates").join("new");
    let template_dst_file = out_dir.join("template.zip");
    println!(
        "Creating template zip: template_dir '{}', destination archive '{}'",
        template_dir.display(),
        template_dst_file.display()
    );
    zip_dir(&template_dir, &template_dst_file, CompressionMethod::Stored).map(|_| {
        println!(
            "Done: {} written to {}",
            template_dir.display(),
            template_dst_file.display()
        );
    })
}

/// Builds the crate in `ink_linting/`. This crate contains the `dylint` driver with ink! specific
/// linting rules.
#[cfg(feature = "cargo-clippy")]
fn build_and_zip_dylint_driver(
    _manifest_dir: PathBuf,
    _out_dir: PathBuf,
    dylint_driver_dst_file: PathBuf,
) -> Result<()> {
    // For `clippy` runs it is not necessary to build the `dylint` driver.
    // Furthermore the fixed Rust nightly specified in `ink_linting/rust-toolchain`
    // contains a bug that results in an `error[E0786]: found invalid metadata files` ICE.
    //
    // We still have to create an empty file though, due to the `include_bytes!` macro.
    File::create(dylint_driver_dst_file)
        .map_err(|err| {
            anyhow::anyhow!(
                "Failed creating an empty ink-dylint-driver.zip file: {:?}",
                err
            )
        })
        .map(|_| ())
}

/// Builds the crate in `ink_linting/`. This crate contains the `dylint` driver with ink! specific
/// linting rules.
#[cfg(not(feature = "cargo-clippy"))]
fn build_and_zip_dylint_driver(
    manifest_dir: PathBuf,
    out_dir: PathBuf,
    dylint_driver_dst_file: PathBuf,
) -> Result<()> {
    let ink_dylint_driver_dir = manifest_dir.join("ink_linting");

    let mut cmd = Command::new("cargo");

    let manifest_arg = format!(
        "--manifest-path={}",
        ink_dylint_driver_dir.join("Cargo.toml").display()
    );
    let target_dir = format!("--target-dir={}", out_dir.display());
    cmd.args(vec![
        "build",
        "--release",
        "--locked",
        &target_dir,
        &manifest_arg,
    ]);

    // We need to remove those environment variables because `dylint` uses a
    // fixed Rust toolchain via the `ink_linting/rust-toolchain` file. By removing
    // these env variables we avoid issues with different Rust toolchains
    // interfering with each other.
    cmd.env_remove("RUSTUP_TOOLCHAIN");
    cmd.env_remove("CARGO_TARGET_DIR");

    println!(
        "Setting cargo working dir to '{}'",
        ink_dylint_driver_dir.display()
    );
    cmd.current_dir(ink_dylint_driver_dir.clone());

    println!("Invoking cargo: {:?}", cmd);

    let child = cmd
        // capture the stdout to return from this function as bytes
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    let output = child.wait_with_output()?;

    if !output.status.success() {
        anyhow::bail!(
            "`{:?}` failed with exit code: {:?}",
            cmd,
            output.status.code()
        );
    }

    println!(
        "Creating ink-dylint-driver.zip: destination archive '{}'",
        dylint_driver_dst_file.display()
    );

    zip_dylint_driver(
        &out_dir.join("release"),
        &dylint_driver_dst_file,
        CompressionMethod::Stored,
    )
    .map(|_| {
        println!(
            "Done: {} written to {}",
            ink_dylint_driver_dir.display(),
            dylint_driver_dst_file.display()
        );
        ()
    })
}

/// Creates a zip archive at `dst_file` with the content of the `src_dir`.
fn zip_dir(src_dir: &Path, dst_file: &Path, method: CompressionMethod) -> Result<()> {
    if !src_dir.exists() {
        anyhow::bail!("src_dir '{}' does not exist", src_dir.display());
    }
    if !src_dir.is_dir() {
        anyhow::bail!("src_dir '{}' is not a directory", src_dir.display());
    }

    let file = File::create(dst_file)?;

    let walkdir = WalkDir::new(src_dir);
    let it = walkdir.into_iter().filter_map(|e| e.ok());

    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(method)
        .unix_permissions(DEFAULT_UNIX_PERMISSIONS);

    let mut buffer = Vec::new();
    for entry in it {
        let path = entry.path();
        let mut name = path.strip_prefix(&src_dir)?.to_path_buf();

        // `Cargo.toml` files cause the folder to excluded from `cargo package` so need to be renamed
        if name.file_name() == Some(OsStr::new("_Cargo.toml")) {
            name.set_file_name("Cargo.toml");
        }

        let file_path = name.as_os_str().to_string_lossy();

        if path.is_file() {
            zip.start_file(file_path, options)?;
            let mut f = File::open(path)?;

            f.read_to_end(&mut buffer)?;
            zip.write_all(&*buffer)?;
            buffer.clear();
        } else if !name.as_os_str().is_empty() {
            zip.add_directory(file_path, options)?;
        }
    }
    zip.finish()?;

    Ok(())
}

/// Creates a zip archive at `dst_file` with the `dylint` driver found in `src_dir`.
///
/// `dylint` drivers have a file name of the form `libink_linting@toolchain.[so,dll]`.
#[cfg(not(feature = "cargo-clippy"))]
fn zip_dylint_driver(src_dir: &Path, dst_file: &Path, method: CompressionMethod) -> Result<()> {
    if !src_dir.exists() {
        anyhow::bail!("src_dir '{}' does not exist", src_dir.display());
    }
    if !src_dir.is_dir() {
        anyhow::bail!("src_dir '{}' is not a directory", src_dir.display());
    }

    let file = File::create(dst_file)?;

    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(method)
        .unix_permissions(DEFAULT_UNIX_PERMISSIONS);

    let mut buffer = Vec::new();

    let walkdir = WalkDir::new(src_dir);
    let it = walkdir.into_iter().filter_map(|e| e.ok());

    for entry in it {
        let path = entry.path();
        let name = path.strip_prefix(&src_dir)?.to_path_buf();
        let file_path = name.as_os_str().to_string_lossy();

        if path.is_file() && path.display().to_string().contains("libink_linting@") {
            zip.start_file(file_path, options)?;
            let mut f = File::open(path)?;

            f.read_to_end(&mut buffer)?;
            zip.write_all(&*buffer)?;
            buffer.clear();

            zip.finish()?;
            break;
        }
    }
    Ok(())
}

/// Generate the `cargo:` key output
fn generate_cargo_keys() {
    let output = Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output();

    let commit = match output {
        Ok(o) if o.status.success() => {
            let sha = String::from_utf8_lossy(&o.stdout).trim().to_owned();
            Cow::from(sha)
        }
        Ok(o) => {
            println!("cargo:warning=Git command failed with status: {}", o.status);
            Cow::from("unknown")
        }
        Err(err) => {
            println!("cargo:warning=Failed to execute git command: {}", err);
            Cow::from("unknown")
        }
    };

    println!(
        "cargo:rustc-env=CARGO_CONTRACT_CLI_IMPL_VERSION={}",
        get_version(&commit)
    )
}

fn get_version(impl_commit: &str) -> String {
    let commit_dash = if impl_commit.is_empty() { "" } else { "-" };

    format!(
        "{}{}{}-{}",
        std::env::var("CARGO_PKG_VERSION").unwrap_or_default(),
        commit_dash,
        impl_commit,
        get_platform(),
    )
}

fn get_platform() -> String {
    let env_dash = if TARGET_ENV.is_some() { "-" } else { "" };

    format!(
        "{}-{}{}{}",
        TARGET_ARCH.as_str(),
        TARGET_OS.as_str(),
        env_dash,
        TARGET_ENV.map(|x| x.as_str()).unwrap_or(""),
    )
}

/// Checks if `dylint-link` is installed, i.e. if the `dylint-link` executable
/// can be executed with a `--version` argument.
fn check_dylint_link_installed() -> Result<()> {
    let execute_cmd = |cmd: &mut Command| {
        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|err| {
                eprintln!("Error spawning `{:?}`", cmd);
                err
            })?
            .wait_with_output()
            .map(|res| {
                let output = format!(
                    "{}",
                    std::str::from_utf8(&res.stdout).expect("conversion must work")
                );
                eprintln!("output: {:?}", output);
                res.status.success()
            })
            .map_err(|err| {
                eprintln!("Error waiting for `{:?}`: {:?}", cmd, err);
                err
            })
    };

    let res = execute_cmd(Command::new("dylint-link").arg("--version"));
    eprintln!("res: {:?}", res);
    if res.is_err() || !res.expect("the `or` case will always will be `Ok`") {
        anyhow::bail!(
            "dylint-link was not found!\n\
            Make sure it is installed and the binary is in your PATH environment.\n\n\
            You can install it by executing `cargo install dylint-link`."
        );
    }
    Ok(())
}

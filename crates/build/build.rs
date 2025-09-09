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

use std::{
    env,
    ffi::OsStr,
    fs::File,
    io::{
        Write,
        prelude::*,
    },
    iter::Iterator,
    path::{
        Path,
        PathBuf,
    },
};

use anyhow::Result;
use walkdir::WalkDir;
use zip::{
    CompressionMethod,
    ZipWriter,
    write::FileOptions,
};

const DEFAULT_UNIX_PERMISSIONS: u32 = 0o755;

fn main() {
    let manifest_dir: PathBuf = env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR should be set by cargo")
        .into();
    let out_dir: PathBuf = env::var("OUT_DIR")
        .expect("OUT_DIR should be set by cargo")
        .into();
    let res = zip_template(&manifest_dir, &out_dir);

    match res {
        Ok(()) => std::process::exit(0),
        Err(err) => {
            eprintln!("Encountered error: {err:?}");
            std::process::exit(1)
        }
    }
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
    let options = FileOptions::<()>::default()
        .compression_method(method)
        .unix_permissions(DEFAULT_UNIX_PERMISSIONS);

    let mut buffer = Vec::new();
    for entry in it {
        let path = entry.path();
        let mut name = path.strip_prefix(src_dir)?.to_path_buf();

        // `Cargo.toml` files cause the folder to excluded from `cargo package` so need to
        // be renamed
        if name.file_name() == Some(OsStr::new("_Cargo.toml")) {
            name.set_file_name("Cargo.toml");
        }

        let file_path = name.as_os_str().to_string_lossy();

        if path.is_file() {
            zip.start_file(file_path, options)?;
            let mut f = File::open(path)?;

            f.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
            buffer.clear();
        } else if !name.as_os_str().is_empty() {
            zip.add_directory(file_path, options)?;
        }
    }
    zip.finish()?;

    Ok(())
}

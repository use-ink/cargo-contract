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

use anyhow::Result;
use heck::ToUpperCamelCase as _;
use std::{
    env,
    fs,
    io::{
        Cursor,
        Read,
        Seek,
        SeekFrom,
        Write,
    },
    path::{
        Path,
        PathBuf,
    },
};

pub fn execute<P>(name: &str, dir: Option<P>) -> Result<()>
where
    P: AsRef<Path>,
{
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        anyhow::bail!(
            "Contract names can only contain alphanumeric characters and underscores"
        );
    }

    if !name
        .chars()
        .next()
        .map(|c| c.is_alphabetic())
        .unwrap_or(false)
    {
        anyhow::bail!("Contract names must begin with an alphabetic character");
    }

    let out_dir = dir
        .map_or(env::current_dir()?, |p| p.as_ref().to_path_buf())
        .join(name);
    if out_dir.join("Cargo.toml").exists() {
        anyhow::bail!("A Cargo package already exists in {}", name);
    }
    if !out_dir.exists() {
        fs::create_dir(&out_dir)?;
    }

    let template = include_bytes!(concat!(env!("OUT_DIR"), "/template.zip"));

    unzip(template, out_dir, Some(name))?;

    Ok(())
}

// Unzips the file at `template` to `out_dir`.
//
// In case `name` is set the zip file is treated as if it were a template for a new
// contract. Replacements in `Cargo.toml` for `name`-placeholders are attempted in
// that case.
fn unzip(template: &[u8], out_dir: PathBuf, name: Option<&str>) -> Result<()> {
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_all(template)?;
    cursor.seek(SeekFrom::Start(0))?;

    let mut archive = zip::ZipArchive::new(cursor)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = out_dir.join(file.name());

        if (*file.name()).ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)?;
                }
            }
            let mut outfile = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(outpath.clone())
                .map_err(|e| {
                    if e.kind() == std::io::ErrorKind::AlreadyExists {
                        anyhow::anyhow!("File {} already exists", file.name(),)
                    } else {
                        anyhow::anyhow!(e)
                    }
                })?;

            if let Some(name) = name {
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                let contents = contents.replace("{{name}}", name);
                let contents =
                    contents.replace("{{camel_name}}", &name.to_upper_camel_case());
                outfile.write_all(contents.as_bytes())?;
            } else {
                let mut v = Vec::new();
                file.read_to_end(&mut v)?;
                outfile.write_all(v.as_slice())?;
            }
        }

        // Get and set permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a temporary directory and passes the `tmp_dir` path to `f`.
    /// Panics if `f` returns an `Err`.
    pub fn with_tmp_dir<F>(f: F)
    where
        F: FnOnce(&Path) -> Result<()>,
    {
        let tmp_dir = tempfile::Builder::new()
            .prefix("contract-template.test.")
            .tempdir()
            .expect("temporary directory creation failed");

        // catch test panics in order to clean up temp dir which will be very large
        f(&tmp_dir.path().canonicalize().unwrap())
            .expect("Error executing test with tmp dir")
    }

    #[test]
    fn rejects_hyphenated_name() {
        with_tmp_dir(|path| {
            let result = execute("rejects-hyphenated-name", Some(path));
            assert!(result.is_err(), "Should fail");
            assert_eq!(
                result.err().unwrap().to_string(),
                "Contract names can only contain alphanumeric characters and underscores"
            );
            Ok(())
        })
    }

    #[test]
    fn rejects_name_with_period() {
        with_tmp_dir(|path| {
            let result = execute("../xxx", Some(path));
            assert!(result.is_err(), "Should fail");
            assert_eq!(
                result.err().unwrap().to_string(),
                "Contract names can only contain alphanumeric characters and underscores"
            );
            Ok(())
        })
    }

    #[test]
    fn rejects_name_beginning_with_number() {
        with_tmp_dir(|path| {
            let result = execute("1xxx", Some(path));
            assert!(result.is_err(), "Should fail");
            assert_eq!(
                result.err().unwrap().to_string(),
                "Contract names must begin with an alphabetic character"
            );
            Ok(())
        })
    }

    #[test]
    fn contract_cargo_project_already_exists() {
        with_tmp_dir(|path| {
            let name = "test_contract_cargo_project_already_exists";
            let _ = execute(name, Some(path));
            let result = execute(name, Some(path));

            assert!(result.is_err(), "Should fail");
            assert_eq!(
                result.err().unwrap().to_string(),
                "A Cargo package already exists in test_contract_cargo_project_already_exists"
            );
            Ok(())
        })
    }

    #[test]
    fn dont_overwrite_existing_files_not_in_cargo_project() {
        with_tmp_dir(|path| {
            let name = "dont_overwrite_existing_files";
            let dir = path.join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::File::create(dir.join(".gitignore")).unwrap();
            let result = execute(name, Some(path));

            assert!(result.is_err(), "Should fail");
            assert_eq!(
                result.err().unwrap().to_string(),
                "File .gitignore already exists"
            );
            Ok(())
        })
    }
}

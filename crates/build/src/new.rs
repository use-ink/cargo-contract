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

use anyhow::Result;
use heck::ToUpperCamelCase as _;
use std::{
    env,
    fs,
    io::Write,
    path::Path,
};

/// Creates a new contract project from the template.
pub fn new_contract_project<P>(name: &str, dir: Option<P>) -> Result<()>
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

    let templates = [
        (
            "rust-toolchain.toml",
            include_str!("../templates/new/rust-toolchain.toml"),
        ),
        ("Cargo.toml", include_str!("../templates/new/_Cargo.toml")),
        ("lib.rs", include_str!("../templates/new/lib.rs")),
        (".gitignore", include_str!("../templates/new/.gitignore")),
    ];
    let camel_name = name.to_upper_camel_case();
    for (file_name, template) in templates {
        let outpath = out_dir.join(file_name);
        let mut outfile = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&outpath)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    anyhow::anyhow!("File {} already exists", file_name)
                } else {
                    anyhow::anyhow!(e)
                }
            })?;
        let contents = template
            .replace("{{name}}", name)
            .replace("{{camel_name}}", &camel_name);
        outfile.write_all(contents.as_bytes())?;

        // Preserve the permissions previously stored in the template archive.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&outpath, fs::Permissions::from_mode(0o755))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::tests::with_tmp_dir;

    #[test]
    fn generated_project_contains_all_template_files() {
        with_tmp_dir(|path| {
            new_contract_project("test_contract", Some(path))?;
            let project = path.join("test_contract");
            let templates = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/new");
            let mut expected = Vec::new();
            for entry in fs::read_dir(templates)? {
                let name = entry?.file_name();
                expected.push(if name == "_Cargo.toml" {
                    "Cargo.toml".into()
                } else {
                    name
                });
            }
            let mut generated = Vec::new();
            for entry in fs::read_dir(project)? {
                let entry = entry?;
                generated.push(entry.file_name());
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    assert_eq!(entry.metadata()?.permissions().mode() & 0o777, 0o755);
                }
            }
            expected.sort();
            generated.sort();
            assert_eq!(generated, expected);
            Ok(())
        })
    }

    #[test]
    fn rejects_hyphenated_name() {
        with_tmp_dir(|path| {
            let result = new_contract_project("rejects-hyphenated-name", Some(path));
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
            let result = new_contract_project("../xxx", Some(path));
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
            let result = new_contract_project("1xxx", Some(path));
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
            new_contract_project(name, Some(path))?;
            let result = new_contract_project(name, Some(path));

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
            for file_name in [".gitignore", "lib.rs", "rust-toolchain.toml"] {
                let name = "dont_overwrite_existing_files";
                let dir = path.join(name);
                fs::create_dir(&dir)?;
                fs::write(dir.join(file_name), "existing contents")?;
                let error = new_contract_project(name, Some(path)).unwrap_err();

                assert_eq!(
                    error.to_string(),
                    format!("File {file_name} already exists")
                );
                assert_eq!(
                    fs::read_to_string(dir.join(file_name))?,
                    "existing contents"
                );
                fs::remove_dir_all(dir)?;
            }
            Ok(())
        })
    }
}

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

use std::{env, fs, path::Path};

use anyhow::Result;

pub(crate) fn execute<P>(name: &str, dir: Option<P>) -> Result<()>
where
    P: AsRef<Path>,
{
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        anyhow::bail!("Contract names can only contain alphanumeric characters and underscores");
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

    crate::util::unzip(template, out_dir, Some(name))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::tests::{with_new_contract_project, with_tmp_dir};

    #[test]
    fn rejects_hyphenated_name() {
        with_new_contract_project(|manifest_path| {
            let result = execute("rejects-hyphenated-name", Some(manifest_path));
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
        with_new_contract_project(|manifest_path| {
            let result = execute("../xxx", Some(manifest_path));
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
        with_new_contract_project(|manifest_path| {
            let result = execute("1xxx", Some(manifest_path));
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

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

use crate::{
    ManifestPath,
    TestCommand,
};
use anyhow::Result;
use std::{
    fs,
    io::Write,
};

macro_rules! test_tests {
    ( $($fn:ident),* ) => {
        #[test]
        fn test_tests() {
            crate::util::tests::with_tmp_dir(|tmp_dir| {
                let ctx = crate::util::tests::CmdTestContext::new(tmp_dir, "build_test")?;
                $( ctx.run_test(stringify!($fn), $fn)?; )*
                Ok(())
            })
        }
    }
}

// All functions provided here are run sequentially as part of the same `#[test]`
// sharing build artifacts (but nothing else) using the [`CmdTestContext`].
//
// The motivation for this is to considerably speed up these tests by only requiring dependencies
// to be build once across all tests.
test_tests!(
    testing_without_flag_must_work,
    testing_without_flag_virtual_workspace_must_work,
    testing_package_flag_must_work,
    testing_package_flag_virtual_workspace_must_work,
    testing_workspace_flag_must_work,
    testing_workspace_flag_virtual_workspace_must_work
);

fn testing_without_flag_must_work(manifest_path: &ManifestPath) -> Result<()> {
    let path = manifest_path.directory().expect("dir must exist");

    // create subcontracts
    let n_contracts = 3;
    let mut project_names = Vec::new();
    for i in 0..n_contracts {
        let project_name = format!("new_project_{}", i);
        crate::cmd::new::execute(&project_name, Some(path))
            .expect("new project creation failed");
        project_names.push(project_name);
    }

    let original_manifest = fs::read_to_string(manifest_path.clone())?;

    // add subcontract to manifest
    let mut manifest_path = path.to_path_buf();
    manifest_path.push("Cargo.toml");
    let mut output = fs::File::create(manifest_path.clone())?;
    write!(output, "[workspace]\n\n")?;
    writeln!(output, "members = [")?;
    for project_name in project_names {
        writeln!(output, "  \"{}\",", project_name)?;
    }
    writeln!(output, "]")?;

    // keep original manifest
    write!(output, "{}", original_manifest)?;

    let cmd = TestCommand {
        manifest_path: Some(manifest_path),
        ..Default::default()
    };

    let results = cmd.exec().expect("test failed");
    assert_eq!(results.len(), 1); // only tests top level contract
    Ok(())
}

fn testing_without_flag_virtual_workspace_must_work(
    manifest_path: &ManifestPath,
) -> Result<()> {
    let path = manifest_path.directory().expect("dir must exist");

    // create subcontracts
    let n_contracts = 3;
    let mut project_names = Vec::new();
    for i in 0..n_contracts {
        let project_name = format!("new_project_{}", i);
        crate::cmd::new::execute(&project_name, Some(path))
            .expect("new project creation failed");
        project_names.push(project_name);
    }

    // delete original lib.rs
    fs::remove_file(path.join("lib.rs")).expect("removal of lib.rs failed");

    // override manifest to create virtual workspace
    let mut manifest_path = path.to_path_buf();
    manifest_path.push("Cargo.toml");
    let mut output = fs::File::create(manifest_path.clone())?;
    write!(output, "[workspace]\n\n")?;
    writeln!(output, "members = [")?;
    for project_name in project_names {
        writeln!(output, "  \"{}\",", project_name)?;
    }
    write!(output, "]")?;

    let cmd = TestCommand {
        manifest_path: Some(manifest_path),
        ..Default::default()
    };

    let results = cmd.exec().expect("test failed");
    assert_eq!(results.len(), n_contracts); // tests all contracts
    Ok(())
}

fn testing_package_flag_must_work(manifest_path: &ManifestPath) -> Result<()> {
    let path = manifest_path.directory().expect("dir must exist");

    let project_name = "new_project";
    crate::cmd::new::execute(project_name, Some(path))
        .expect("new project creation failed");

    let original_manifest = fs::read_to_string(manifest_path.clone())?;

    // add subcontract to manifest
    let mut manifest_path = path.to_path_buf();
    manifest_path.push("Cargo.toml");
    let mut output = fs::File::create(manifest_path.clone())?;
    write!(output, "[workspace]\n\n")?;
    writeln!(output, "members = [")?;
    writeln!(output, "  \"{}\",", project_name)?;
    writeln!(output, "]")?;

    // keep original manifest
    write!(output, "{}", original_manifest)?;

    let cmd = TestCommand {
        package: Some(project_name.to_string()),
        manifest_path: Some(manifest_path),
        ..Default::default()
    };

    let results = cmd.exec().expect("test failed");
    assert_eq!(results.len(), 1); // only tests specified package
    Ok(())
}

fn testing_package_flag_virtual_workspace_must_work(
    manifest_path: &ManifestPath,
) -> Result<()> {
    let path = manifest_path.directory().expect("dir must exist");

    let project_name = "new_project";
    crate::cmd::new::execute(project_name, Some(path))
        .expect("new project creation failed");

    // delete original lib.rs
    fs::remove_file(path.join("lib.rs")).expect("removal of lib.rs failed");

    // override manifest to create virtual workspace
    let mut manifest_path = path.to_path_buf();
    manifest_path.push("Cargo.toml");
    let mut output = fs::File::create(manifest_path.clone())?;
    write!(output, "[workspace]\n\n")?;
    writeln!(output, "members = [")?;
    writeln!(output, "  \"{}\",", project_name)?;
    write!(output, "]")?;

    let cmd = TestCommand {
        package: Some(project_name.to_string()),
        manifest_path: Some(manifest_path),
        ..Default::default()
    };

    let results = cmd.exec().expect("test failed");
    assert_eq!(results.len(), 1); // only tests specified package
    Ok(())
}

fn testing_workspace_flag_must_work(manifest_path: &ManifestPath) -> Result<()> {
    let path = manifest_path.directory().expect("dir must exist");

    // create subcontracts
    let n_contracts = 3;
    let mut project_names = Vec::new();
    for i in 0..n_contracts {
        let project_name = format!("new_project_{}", i);
        crate::cmd::new::execute(&project_name, Some(path))
            .expect("new project creation failed");
        project_names.push(project_name);
    }

    let original_manifest = fs::read_to_string(manifest_path.clone())?;

    // add subcontract to manifest
    let mut manifest_path = path.to_path_buf();
    manifest_path.push("Cargo.toml");
    let mut output = fs::File::create(manifest_path.clone())?;
    write!(output, "[workspace]\n\n")?;
    writeln!(output, "members = [")?;
    for project_name in project_names {
        writeln!(output, "  \"{}\",", project_name)?;
    }
    writeln!(output, "]")?;

    // keep original manifest
    write!(output, "{}", original_manifest)?;

    let cmd = TestCommand {
        test_workspace: true,
        manifest_path: Some(manifest_path),
        ..Default::default()
    };

    let results = cmd.exec().expect("test failed");
    assert_eq!(results.len(), n_contracts + 1); // tests all subcontracts plus top level contract
    Ok(())
}

fn testing_workspace_flag_virtual_workspace_must_work(
    manifest_path: &ManifestPath,
) -> Result<()> {
    let path = manifest_path.directory().expect("dir must exist");

    // create subcontracts
    let n_contracts = 3;
    let mut project_names = Vec::new();
    for i in 0..n_contracts {
        let project_name = format!("new_project_{}", i);
        crate::cmd::new::execute(&project_name, Some(path))
            .expect("new project creation failed");
        project_names.push(project_name);
    }

    // delete original lib.rs
    fs::remove_file(path.join("lib.rs")).expect("removal of lib.rs failed");

    // override manifest to create virtual workspace
    let mut manifest_path = path.to_path_buf();
    manifest_path.push("Cargo.toml");
    let mut output = fs::File::create(manifest_path.clone())?;
    write!(output, "[workspace]\n\n")?;
    writeln!(output, "members = [")?;
    for project_name in project_names {
        writeln!(output, "  \"{}\",", project_name)?;
    }
    write!(output, "]")?;

    let cmd = TestCommand {
        test_workspace: true,
        manifest_path: Some(manifest_path),
        ..Default::default()
    };

    let results = cmd.exec().expect("test failed");
    assert_eq!(results.len(), n_contracts); // tests all contracts
    Ok(())
}

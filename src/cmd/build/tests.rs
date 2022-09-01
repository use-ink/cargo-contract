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
    cmd::{
        build::load_module,
        BuildCommand,
    },
    util::tests::{
        BuildTestContext,
        TestContractManifest,
    },
    BuildArtifacts,
    BuildMode,
    ManifestPath,
    OptimizationPasses,
    OutputType,
    UnstableOptions,
    VerbosityFlags,
};
use anyhow::Result;
use contract_metadata::*;
use serde_json::{
    Map,
    Value,
};
use std::{
    ffi::OsStr,
    fmt::Write,
    fs,
    path::Path,
};

#[test]
fn build_tests() {
    crate::util::tests::with_tmp_dir(|tmp_dir| {
        let ctx = BuildTestContext::new(tmp_dir, "build_test")?;

        ctx.run_test("build_code_only", build_code_only)?;
        ctx.run_test(
            "check_must_not_output_contract_artifacts_in_project_dir",
            check_must_not_output_contract_artifacts_in_project_dir,
        )?;
        ctx.run_test(
            "optimization_passes_from_cli_must_take_precedence_over_profile",
            optimization_passes_from_cli_must_take_precedence_over_profile,
        )?;
        ctx.run_test(
            "optimization_passes_from_profile_must_be_used",
            optimization_passes_from_profile_must_be_used,
        )?;
        ctx.run_test(
            "building_template_in_debug_mode_must_work",
            contract_lib_name_different_from_package_name_must_build,
        )?;
        ctx.run_test(
            "building_template_in_debug_mode_must_work",
            building_template_in_debug_mode_must_work,
        )?;
        ctx.run_test(
            "building_template_in_release_mode_must_work",
            building_template_in_release_mode_must_work,
        )?;
        ctx.run_test(
            "keep_debug_symbols_in_debug_mode",
            keep_debug_symbols_in_debug_mode,
        )?;
        ctx.run_test(
            "keep_debug_symbols_in_release_mode",
            keep_debug_symbols_in_release_mode,
        )?;
        ctx.run_test(
            "check_must_not_output_contract_artifacts_in_project_dir",
            build_with_json_output_works,
        )?;
        ctx.run_test(
            "building_contract_with_source_file_in_subfolder_must_work",
            building_contract_with_source_file_in_subfolder_must_work,
        )?;
        #[cfg(unix)]
        ctx.run_test(
            "missing_cargo_dylint_installation_must_be_detected",
            missing_cargo_dylint_installation_must_be_detected,
        )?;
        ctx.run_test("generates_metadata", generates_metadata)?;
        Ok(())
    })
}

fn build_code_only(manifest_path: &ManifestPath) -> Result<()> {
    let args = crate::cmd::build::ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_mode: BuildMode::Release,
        build_artifact: BuildArtifacts::CodeOnly,
        skip_linting: true,
        ..Default::default()
    };

    let res = super::execute(args).expect("build failed");

    // our ci has set `CARGO_TARGET_DIR` to cache artifacts.
    // this dir does not include `/target/` as a path, hence
    // we can't match for e.g. `foo_project/target/ink`.
    //
    // we also can't match for `/ink` here, since this would match
    // for `/ink` being the root path.
    assert!(res.target_directory.ends_with("ink"));

    assert!(
        res.metadata_result.is_none(),
        "CodeOnly should not generate the metadata"
    );

    let optimized_size = res.optimization_result.unwrap().optimized_size;
    assert!(optimized_size > 0.0);

    // our optimized contract template should always be below 3k.
    assert!(optimized_size < 3.0);

    // we specified that debug symbols should be removed
    // original code should have some but the optimized version should have them removed
    assert!(!has_debug_symbols(&res.dest_wasm.unwrap()));

    Ok(())
}

fn check_must_not_output_contract_artifacts_in_project_dir(
    manifest_path: &ManifestPath,
) -> Result<()> {
    // given
    let project_dir = manifest_path.directory().expect("directory must exist");
    let args = crate::cmd::build::ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_artifact: BuildArtifacts::CheckOnly,
        skip_linting: true,
        ..Default::default()
    };

    // when
    super::execute(args).expect("build failed");

    // then
    assert!(
        !project_dir.join("target/ink/new_project.contract").exists(),
        "found contract artifact in project directory!"
    );
    assert!(
        !project_dir.join("target/ink/new_project.wasm").exists(),
        "found wasm artifact in project directory!"
    );
    Ok(())
}

fn optimization_passes_from_cli_must_take_precedence_over_profile(
    manifest_path: &ManifestPath,
) -> Result<()> {
    // given
    let mut test_manifest = TestContractManifest::new(manifest_path.clone())?;
    test_manifest.set_profile_optimization_passes(OptimizationPasses::Three)?;
    test_manifest.write()?;

    let cmd = BuildCommand {
        manifest_path: Some(manifest_path.as_ref().into()),
        build_artifact: BuildArtifacts::All,
        build_release: false,
        build_offline: false,
        verbosity: VerbosityFlags::default(),
        unstable_options: UnstableOptions::default(),

        // we choose zero optimization passes as the "cli" parameter
        optimization_passes: Some(OptimizationPasses::Zero),
        keep_debug_symbols: false,
        skip_linting: true,
        output_json: false,
    };

    // when
    let res = cmd.exec().expect("build failed");
    let optimization = res
        .optimization_result
        .expect("no optimization result available");

    // then
    // The size does not exactly match the original size even without optimization
    // passed because there is still some post processing happening.
    let size_diff = optimization.original_size - optimization.optimized_size;
    assert!(
        0.0 < size_diff && size_diff < 10.0,
        "The optimized size savings are larger than allowed or negative: {}",
        size_diff,
    );
    Ok(())
}

fn optimization_passes_from_profile_must_be_used(
    manifest_path: &ManifestPath,
) -> Result<()> {
    // given
    let mut test_manifest = TestContractManifest::new(manifest_path.clone())?;
    test_manifest.set_profile_optimization_passes(OptimizationPasses::Three)?;
    test_manifest.write()?;

    let cmd = BuildCommand {
        manifest_path: Some(manifest_path.as_ref().into()),
        build_artifact: BuildArtifacts::All,
        build_release: false,
        build_offline: false,
        verbosity: VerbosityFlags::default(),
        unstable_options: UnstableOptions::default(),

        // we choose no optimization passes as the "cli" parameter
        optimization_passes: None,
        keep_debug_symbols: false,
        skip_linting: true,
        output_json: false,
    };

    // when
    let res = cmd.exec().expect("build failed");
    let optimization = res
        .optimization_result
        .expect("no optimization result available");

    // then
    // The size does not exactly match the original size even without optimization
    // passed because there is still some post processing happening.
    let size_diff = optimization.original_size - optimization.optimized_size;
    assert!(
        size_diff > (optimization.original_size / 2.0),
        "The optimized size savings are too small: {}",
        size_diff,
    );

    Ok(())
}

fn contract_lib_name_different_from_package_name_must_build(
    manifest_path: &ManifestPath,
) -> Result<()> {
    // given
    let mut manifest = TestContractManifest::new(manifest_path.clone())?;
    manifest.set_lib_name("some_lib_name")?;
    manifest.set_package_name("some_package_name")?;
    manifest.write()?;

    // when
    let cmd = BuildCommand {
        manifest_path: Some(manifest_path.as_ref().into()),
        build_artifact: BuildArtifacts::All,
        build_release: false,
        build_offline: false,
        verbosity: VerbosityFlags::default(),
        unstable_options: UnstableOptions::default(),
        optimization_passes: None,
        keep_debug_symbols: false,
        skip_linting: true,
        output_json: false,
    };
    let res = cmd.exec().expect("build failed");

    // then
    assert_eq!(
        res.dest_wasm
            .expect("`dest_wasm` does not exist")
            .file_name(),
        Some(OsStr::new("some_lib_name.wasm"))
    );

    Ok(())
}

fn building_template_in_debug_mode_must_work(manifest_path: &ManifestPath) -> Result<()> {
    // given
    let args = crate::cmd::build::ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_mode: BuildMode::Debug,
        skip_linting: true,
        ..Default::default()
    };

    // when
    let res = super::execute(args);

    // then
    assert!(res.is_ok(), "building template in debug mode failed!");
    Ok(())
}

fn building_template_in_release_mode_must_work(
    manifest_path: &ManifestPath,
) -> Result<()> {
    // given
    let args = crate::cmd::build::ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_mode: BuildMode::Release,
        skip_linting: true,
        ..Default::default()
    };

    // when
    let res = super::execute(args);

    // then
    assert!(res.is_ok(), "building template in release mode failed!");
    Ok(())
}

fn building_contract_with_source_file_in_subfolder_must_work(
    manifest_path: &ManifestPath,
) -> Result<()> {
    // given
    let path = manifest_path.directory().expect("dir must exist");
    let old_lib_path = path.join(Path::new("lib.rs"));
    let new_lib_path = path.join(Path::new("srcfoo")).join(Path::new("lib.rs"));
    let new_dir_path = path.join(Path::new("srcfoo"));
    fs::create_dir_all(new_dir_path).expect("creating dir must work");
    fs::rename(old_lib_path, new_lib_path).expect("moving file must work");

    let mut manifest = TestContractManifest::new(manifest_path.clone())?;
    manifest.set_lib_path("srcfoo/lib.rs")?;
    manifest.write()?;

    let args = crate::cmd::build::ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_artifact: BuildArtifacts::CheckOnly,
        skip_linting: true,
        ..Default::default()
    };

    // when
    let res = super::execute(args);

    // then
    assert!(res.is_ok(), "building contract failed!");
    Ok(())
}

fn keep_debug_symbols_in_debug_mode(manifest_path: &ManifestPath) -> Result<()> {
    let args = crate::cmd::build::ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_mode: BuildMode::Debug,
        build_artifact: BuildArtifacts::CodeOnly,
        keep_debug_symbols: true,
        skip_linting: true,
        ..Default::default()
    };

    let res = super::execute(args).expect("build failed");

    // we specified that debug symbols should be kept
    assert!(has_debug_symbols(&res.dest_wasm.unwrap()));

    Ok(())
}

fn keep_debug_symbols_in_release_mode(manifest_path: &ManifestPath) -> Result<()> {
    let args = crate::cmd::build::ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_mode: BuildMode::Release,
        build_artifact: BuildArtifacts::CodeOnly,
        keep_debug_symbols: true,
        skip_linting: true,
        ..Default::default()
    };

    let res = super::execute(args).expect("build failed");

    // we specified that debug symbols should be kept
    assert!(has_debug_symbols(&res.dest_wasm.unwrap()));

    Ok(())
}

fn build_with_json_output_works(manifest_path: &ManifestPath) -> Result<()> {
    // given
    let args = crate::cmd::build::ExecuteArgs {
        manifest_path: manifest_path.clone(),
        output_type: OutputType::Json,
        skip_linting: true,
        ..Default::default()
    };

    // when
    let res = super::execute(args).expect("build failed");

    // then
    assert!(res.serialize_json().is_ok());
    Ok(())
}

fn missing_cargo_dylint_installation_must_be_detected(
    manifest_path: &ManifestPath,
) -> Result<()> {
    use super::util::tests::create_executable;

    // given
    let manifest_dir = manifest_path.directory().unwrap();

    // mock existing `dylint-link` binary
    let _tmp0 = create_executable(&manifest_dir.join("dylint-link"), "#!/bin/sh\nexit 0");

    // mock a non-existing `cargo dylint` installation.
    let _tmp1 = create_executable(&manifest_dir.join("cargo"), "#!/bin/sh\nexit 1");

    // when
    let args = crate::cmd::build::ExecuteArgs {
        manifest_path: manifest_path.clone(),
        ..Default::default()
    };
    let res = super::execute(args).map(|_| ()).unwrap_err();

    // then
    assert!(format!("{:?}", res).contains("cargo-dylint was not found!"));

    Ok(())
}

fn generates_metadata(manifest_path: &ManifestPath) -> Result<()> {
    // add optional metadata fields
    let mut test_manifest = TestContractManifest::new(manifest_path.clone())?;
    test_manifest.add_package_value("description", "contract description".into())?;
    test_manifest
        .add_package_value("documentation", "http://documentation.com".into())?;
    test_manifest.add_package_value("repository", "http://repository.com".into())?;
    test_manifest.add_package_value("homepage", "http://homepage.com".into())?;
    test_manifest.add_package_value("license", "Apache-2.0".into())?;
    test_manifest
        .add_user_metadata_value("some-user-provided-field", "and-its-value".into())?;
    test_manifest.add_user_metadata_value(
        "more-user-provided-fields",
        vec!["and", "their", "values"].into(),
    )?;
    test_manifest.write()?;

    let crate_metadata = crate::crate_metadata::CrateMetadata::collect(manifest_path)?;

    // usually this file will be produced by a previous build step
    let final_contract_wasm_path = &crate_metadata.dest_wasm;
    fs::create_dir_all(final_contract_wasm_path.parent().unwrap()).unwrap();
    fs::write(final_contract_wasm_path, "TEST FINAL WASM BLOB").unwrap();

    let mut args = crate::cmd::build::ExecuteArgs {
        skip_linting: true,
        ..Default::default()
    };
    args.manifest_path = manifest_path.clone();

    let build_result = crate::cmd::build::execute(args)?;
    let dest_bundle = build_result
        .metadata_result
        .expect("Metadata should be generated")
        .dest_bundle;

    let metadata_json: Map<String, Value> =
        serde_json::from_slice(&fs::read(&dest_bundle)?)?;

    assert!(
        dest_bundle.exists(),
        "Missing metadata file '{}'",
        dest_bundle.display()
    );

    let source = metadata_json.get("source").expect("source not found");
    let hash = source.get("hash").expect("source.hash not found");
    let language = source.get("language").expect("source.language not found");
    let compiler = source.get("compiler").expect("source.compiler not found");
    let wasm = source.get("wasm").expect("source.wasm not found");

    let contract = metadata_json.get("contract").expect("contract not found");
    let name = contract.get("name").expect("contract.name not found");
    let version = contract.get("version").expect("contract.version not found");
    let authors = contract
        .get("authors")
        .expect("contract.authors not found")
        .as_array()
        .expect("contract.authors is an array")
        .iter()
        .map(|author| author.as_str().expect("author is a string"))
        .collect::<Vec<_>>();
    let description = contract
        .get("description")
        .expect("contract.description not found");
    let documentation = contract
        .get("documentation")
        .expect("contract.documentation not found");
    let repository = contract
        .get("repository")
        .expect("contract.repository not found");
    let homepage = contract
        .get("homepage")
        .expect("contract.homepage not found");
    let license = contract.get("license").expect("contract.license not found");

    let user = metadata_json.get("user").expect("user section not found");

    // calculate wasm hash
    let fs_wasm = fs::read(&crate_metadata.dest_wasm)?;
    let expected_hash = crate::cmd::metadata::blake2_hash(&fs_wasm[..]);
    let expected_wasm = build_byte_str(&fs_wasm);

    let expected_language =
        SourceLanguage::new(Language::Ink, crate_metadata.ink_version).to_string();
    let expected_rustc_version =
        semver::Version::parse(&rustc_version::version()?.to_string())?;
    let expected_compiler =
        SourceCompiler::new(Compiler::RustC, expected_rustc_version).to_string();
    let mut expected_user_metadata = serde_json::Map::new();
    expected_user_metadata
        .insert("some-user-provided-field".into(), "and-its-value".into());
    expected_user_metadata.insert(
        "more-user-provided-fields".into(),
        serde_json::Value::Array(vec!["and".into(), "their".into(), "values".into()]),
    );

    assert_eq!(build_byte_str(&expected_hash.0[..]), hash.as_str().unwrap());
    assert_eq!(expected_wasm, wasm.as_str().unwrap());
    assert_eq!(expected_language, language.as_str().unwrap());
    assert_eq!(expected_compiler, compiler.as_str().unwrap());
    assert_eq!(
        crate_metadata.contract_artifact_name,
        name.as_str().unwrap()
    );
    assert_eq!(
        crate_metadata.root_package.version.to_string(),
        version.as_str().unwrap()
    );
    assert_eq!(crate_metadata.root_package.authors, authors);
    assert_eq!("contract description", description.as_str().unwrap());
    assert_eq!("http://documentation.com/", documentation.as_str().unwrap());
    assert_eq!("http://repository.com/", repository.as_str().unwrap());
    assert_eq!("http://homepage.com/", homepage.as_str().unwrap());
    assert_eq!("Apache-2.0", license.as_str().unwrap());
    assert_eq!(&expected_user_metadata, user.as_object().unwrap());

    Ok(())
}

fn build_byte_str(bytes: &[u8]) -> String {
    let mut str = String::new();
    write!(str, "0x").expect("failed writing to string");
    for byte in bytes {
        write!(str, "{:02x}", byte).expect("failed writing to string");
    }
    str
}

fn has_debug_symbols<P: AsRef<Path>>(p: P) -> bool {
    load_module(p)
        .unwrap()
        .custom_sections()
        .any(|e| e.name() == "name")
}

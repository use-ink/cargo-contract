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
    util::tests::TestContractManifest,
    BuildArtifacts,
    BuildMode,
    BuildResult,
    CrateMetadata,
    ExecuteArgs,
    ManifestPath,
    OptimizationPasses,
    OutputType,
    Target,
    Verbosity,
};
use anyhow::Result;
use contract_metadata::*;
use serde_json::{
    Map,
    Value,
};
use std::{
    fmt::Write,
    fs,
    path::{
        Path,
        PathBuf,
    },
    time::SystemTime,
};

macro_rules! build_tests {
    ( $($fn:ident),* ) => {
        #[test]
        fn build_tests() -> Result<()> {
            let tmp_dir = ::tempfile::Builder::new()
                .prefix("cargo-contract-build.test.")
                .tempdir()
                .expect("temporary directory creation failed");

            let ctx = crate::tests::BuildTestContext::new(tmp_dir.path(), "build_test")?;
            $( ctx.run_test(stringify!($fn), $fn)?; )*
            Ok(())
        }
    }
}

// All functions provided here are run sequentially as part of the same `#[test]`
// sharing build artifacts (but nothing else) using the [`BuildTestContext`].
//
// The motivation for this is to considerably speed up these tests by only requiring
// dependencies to be build once across all tests.
build_tests!(
    build_code_only,
    check_must_not_output_contract_artifacts_in_project_dir,
    optimization_passes_from_cli_must_take_precedence_over_profile,
    optimization_passes_from_profile_must_be_used,
    building_template_in_debug_mode_must_work,
    building_template_in_release_mode_must_work,
    keep_debug_symbols_in_debug_mode,
    keep_debug_symbols_in_release_mode,
    build_with_json_output_works,
    building_contract_with_source_file_in_subfolder_must_work,
    building_contract_with_build_rs_must_work,
    missing_cargo_dylint_installation_must_be_detected,
    generates_metadata,
    unchanged_contract_skips_optimization_and_metadata_steps,
    unchanged_contract_no_metadata_artifacts_generates_metadata
);

fn build_code_only(manifest_path: &ManifestPath) -> Result<()> {
    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_mode: BuildMode::Release,
        build_artifact: BuildArtifacts::CodeOnly,
        lint: false,
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
    assert!(!has_debug_symbols(res.dest_wasm.unwrap()));

    Ok(())
}

fn check_must_not_output_contract_artifacts_in_project_dir(
    manifest_path: &ManifestPath,
) -> Result<()> {
    // given
    let project_dir = manifest_path.directory().expect("directory must exist");
    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_artifact: BuildArtifacts::CheckOnly,
        lint: false,
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

    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        verbosity: Verbosity::Default,
        features: Default::default(),
        build_mode: Default::default(),
        network: Default::default(),
        build_artifact: BuildArtifacts::All,
        unstable_flags: Default::default(),
        optimization_passes: Some(OptimizationPasses::Zero),
        keep_debug_symbols: false,
        lint: false,
        output_type: OutputType::Json,
        skip_wasm_validation: false,
        target: Default::default(),
        ..Default::default()
    };

    // when
    let res = crate::execute(args).expect("build failed");
    let optimization = res
        .optimization_result
        .expect("no optimization result available");

    // then
    // The size does not exactly match the original size even without optimization
    // passed because there is still some post processing happening.
    let size_diff = optimization.original_size - optimization.optimized_size;
    assert!(
        0.0 < size_diff && size_diff < 10.0,
        "The optimized size savings are larger than allowed or negative: {size_diff}",
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

    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        verbosity: Verbosity::Default,
        features: Default::default(),
        build_mode: Default::default(),
        network: Default::default(),
        build_artifact: BuildArtifacts::All,
        unstable_flags: Default::default(),
        // no optimization passes specified.
        optimization_passes: None,
        keep_debug_symbols: false,
        lint: false,
        output_type: OutputType::Json,
        skip_wasm_validation: false,
        target: Default::default(),
        ..Default::default()
    };

    // when
    let res = crate::execute(args).expect("build failed");
    let optimization = res
        .optimization_result
        .expect("no optimization result available");

    // then
    // The size does not exactly match the original size even without optimization
    // passed because there is still some post processing happening.
    let size_diff = optimization.original_size - optimization.optimized_size;
    assert!(
        size_diff > (optimization.original_size / 2.0),
        "The optimized size savings are too small: {size_diff}",
    );

    Ok(())
}

fn building_template_in_debug_mode_must_work(manifest_path: &ManifestPath) -> Result<()> {
    // given
    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_mode: BuildMode::Debug,
        lint: false,
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
    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_mode: BuildMode::Release,
        lint: false,
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

    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_artifact: BuildArtifacts::CheckOnly,
        lint: false,
        ..Default::default()
    };

    // when
    let res = super::execute(args);

    // then
    assert!(res.is_ok(), "building contract failed!");
    Ok(())
}

fn building_contract_with_build_rs_must_work(manifest_path: &ManifestPath) -> Result<()> {
    // given
    let mut test_manifest = TestContractManifest::new(manifest_path.clone())?;
    test_manifest.add_package_value("build", "build.rs".to_string().into())?;
    test_manifest.write()?;

    let path = manifest_path.directory().expect("dir must exist");
    let build_rs_path = path.join(Path::new("build.rs"));

    fs::write(build_rs_path, "fn main() {}")?;

    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_artifact: BuildArtifacts::CheckOnly,
        lint: false,
        ..Default::default()
    };

    // when
    let res = super::execute(args);

    // then
    assert!(res.is_ok(), "building contract failed!");
    Ok(())
}

fn keep_debug_symbols_in_debug_mode(manifest_path: &ManifestPath) -> Result<()> {
    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_mode: BuildMode::Debug,
        build_artifact: BuildArtifacts::CodeOnly,
        keep_debug_symbols: true,
        lint: false,
        ..Default::default()
    };

    let res = super::execute(args).expect("build failed");

    // we specified that debug symbols should be kept
    assert!(has_debug_symbols(res.dest_wasm.unwrap()));

    Ok(())
}

fn keep_debug_symbols_in_release_mode(manifest_path: &ManifestPath) -> Result<()> {
    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_mode: BuildMode::Release,
        build_artifact: BuildArtifacts::CodeOnly,
        keep_debug_symbols: true,
        lint: false,
        ..Default::default()
    };

    let res = super::execute(args).expect("build failed");

    // we specified that debug symbols should be kept
    assert!(has_debug_symbols(res.dest_wasm.unwrap()));

    Ok(())
}

fn build_with_json_output_works(manifest_path: &ManifestPath) -> Result<()> {
    // given
    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        output_type: OutputType::Json,
        lint: false,
        ..Default::default()
    };

    // when
    let res = super::execute(args).expect("build failed");

    // then
    assert!(res.serialize_json().is_ok());
    Ok(())
}

#[cfg(unix)]
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
    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        lint: true,
        ..Default::default()
    };
    let res = super::execute(args).map(|_| ()).unwrap_err();

    // then
    assert!(format!("{res:?}").contains("cargo-dylint was not found!"));

    Ok(())
}

#[cfg(not(unix))]
fn missing_cargo_dylint_installation_must_be_detected(
    _manifest_path: &ManifestPath,
) -> Result<()> {
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

    let crate_metadata = CrateMetadata::collect(manifest_path, Target::Wasm)?;

    // usually this file will be produced by a previous build step
    let final_contract_wasm_path = &crate_metadata.dest_code;
    fs::create_dir_all(final_contract_wasm_path.parent().unwrap()).unwrap();
    fs::write(final_contract_wasm_path, "TEST FINAL WASM BLOB").unwrap();

    let mut args = ExecuteArgs {
        lint: false,
        ..Default::default()
    };
    args.manifest_path = manifest_path.clone();

    let build_result = crate::execute(args)?;
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
    let fs_wasm = fs::read(&crate_metadata.dest_code)?;
    let expected_hash = crate::code_hash(&fs_wasm[..]);
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

    assert_eq!(build_byte_str(&expected_hash[..]), hash.as_str().unwrap());
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

fn unchanged_contract_skips_optimization_and_metadata_steps(
    manifest_path: &ManifestPath,
) -> Result<()> {
    // given
    let args = ExecuteArgs {
        manifest_path: manifest_path.clone(),
        ..Default::default()
    };

    fn get_last_modified(res: &BuildResult) -> (SystemTime, SystemTime, SystemTime) {
        assert!(
            res.dest_wasm.is_some(),
            "dest_wasm should always be returned for a full build"
        );
        assert!(
            res.metadata_result.is_some(),
            "metadata_result should always be returned for a full build"
        );
        let dest_wasm_modified = file_last_modified(res.dest_wasm.as_ref().unwrap());
        let metadata_result_modified =
            file_last_modified(&res.metadata_result.as_ref().unwrap().dest_metadata);
        let contract_bundle_modified =
            file_last_modified(&res.metadata_result.as_ref().unwrap().dest_bundle);
        (
            dest_wasm_modified,
            metadata_result_modified,
            contract_bundle_modified,
        )
    }

    // when
    let res1 = super::execute(args.clone()).expect("build failed");
    let (opt_result_modified1, metadata_modified1, contract_bundle_modified1) =
        get_last_modified(&res1);
    let res2 = super::execute(args).expect("build failed");
    let (opt_result_modified2, metadata_modified2, contract_bundle_modified2) =
        get_last_modified(&res2);

    // then
    assert_eq!(
        opt_result_modified1, opt_result_modified2,
        "Subsequent build of unchanged contract should not perform optimization"
    );
    assert_eq!(
        metadata_modified1, metadata_modified2,
        "Subsequent build of unchanged contract should not perform metadata generation"
    );
    assert_eq!(contract_bundle_modified1, contract_bundle_modified2, "Subsequent build of unchanged contract should not perform contract bundle generation");

    Ok(())
}

fn unchanged_contract_no_metadata_artifacts_generates_metadata(
    manifest_path: &ManifestPath,
) -> Result<()> {
    let res1 = super::execute(ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_artifact: BuildArtifacts::CodeOnly,
        ..Default::default()
    })
    .expect("build failed");

    // CodeOnly should only generate Wasm code artifact
    assert!(res1.dest_wasm.as_ref().unwrap().exists());
    assert!(res1.metadata_result.is_none());

    let dest_wasm_modified_pre = file_last_modified(&res1.dest_wasm.unwrap());

    let res2 = super::execute(ExecuteArgs {
        manifest_path: manifest_path.clone(),
        build_artifact: BuildArtifacts::All,
        ..Default::default()
    })
    .expect("build failed");

    let dest_wasm_modified_post = file_last_modified(res2.dest_wasm.as_ref().unwrap());

    // Code remains unchanged, but metadata artifacts are now generated
    assert_eq!(dest_wasm_modified_pre, dest_wasm_modified_post);
    assert!(
        res2.metadata_result
            .as_ref()
            .unwrap()
            .dest_metadata
            .exists(),
        "Metadata file should have been generated"
    );
    assert!(
        res2.metadata_result.as_ref().unwrap().dest_bundle.exists(),
        "Contract bundle should have been generated"
    );

    Ok(())
}

/// Get the last modified date of the given file.
/// Panics if the file does not exist.
fn file_last_modified(path: &Path) -> SystemTime {
    fs::metadata(path)
        .unwrap_or_else(|err| {
            panic!("Failed to read metadata for '{}': {}", path.display(), err)
        })
        .modified()
        .unwrap_or_else(|err| {
            panic!(
                "Failed to read modified time for '{}': {}",
                path.display(),
                err
            )
        })
}

fn build_byte_str(bytes: &[u8]) -> String {
    let mut str = String::new();
    write!(str, "0x").expect("failed writing to string");
    for byte in bytes {
        write!(str, "{byte:02x}").expect("failed writing to string");
    }
    str
}

fn has_debug_symbols<P: AsRef<Path>>(p: P) -> bool {
    crate::load_module(p)
        .unwrap()
        .custom_sections()
        .any(|e| e.name() == "name")
}

/// Enables running a group of tests sequentially, each starting with the original
/// template contract, but maintaining the target directory so compilation artifacts are
/// maintained across each test.
pub struct BuildTestContext {
    template_dir: PathBuf,
    working_dir: PathBuf,
}

impl BuildTestContext {
    /// Create a new `BuildTestContext`, running the `new` command to create a blank
    /// contract template project for testing the build process.
    pub fn new(tmp_dir: &Path, working_project_name: &str) -> Result<Self> {
        crate::new_contract_project(working_project_name, Some(tmp_dir))
            .expect("new project creation failed");
        let working_dir = tmp_dir.join(working_project_name);

        let template_dir = tmp_dir.join(format!("{working_project_name}_template"));

        fs::rename(&working_dir, &template_dir)?;
        copy_dir_all(&template_dir, &working_dir)?;

        Ok(Self {
            template_dir,
            working_dir,
        })
    }

    /// Run the supplied test. Test failure will print the error to `stdout`, and this
    /// will still return `Ok(())` in order that subsequent tests will still be run.
    ///
    /// The test may modify the contracts project files (e.g. Cargo.toml, lib.rs), so
    /// after completion those files are reverted to their original state for the next
    /// test.
    ///
    /// Importantly, the `target` directory is maintained so as to avoid recompiling all
    /// of the dependencies for each test.
    pub fn run_test(
        &self,
        name: &str,
        test: impl FnOnce(&ManifestPath) -> Result<()>,
    ) -> Result<()> {
        println!("Running {name}");
        let manifest_path = ManifestPath::new(self.working_dir.join("Cargo.toml"))?;
        let crate_metadata = CrateMetadata::collect(&manifest_path, Target::Wasm)?;
        match test(&manifest_path) {
            Ok(()) => (),
            Err(err) => {
                println!("{name} FAILED: {err:?}");
            }
        }
        // revert to the original template files, but keep the `target` dir from the
        // previous run.
        self.remove_all_except_target_dir()?;
        copy_dir_all(&self.template_dir, &self.working_dir)?;
        // remove the original wasm artifact to force it to be rebuilt
        if crate_metadata.original_code.exists() {
            fs::remove_file(&crate_metadata.original_code)?;
        }
        Ok(())
    }

    /// Deletes all files and folders in project dir (except the `target` directory)
    fn remove_all_except_target_dir(&self) -> Result<()> {
        for entry in fs::read_dir(&self.working_dir)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_dir() {
                // remove all except the target dir
                if entry.file_name() != "target" {
                    fs::remove_dir_all(entry.path())?
                }
            } else {
                fs::remove_file(entry.path())?
            }
        }
        Ok(())
    }
}

/// Copy contents of `src` to `dst` recursively.
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

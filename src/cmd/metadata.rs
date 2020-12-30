// Copyright 2018-2020 Parity Technologies (UK) Ltd.
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
    crate_metadata::CrateMetadata,
    util,
    workspace::{ManifestPath, Workspace},
    BuildArtifacts, BuildResult, OptimizationResult, UnstableFlags, Verbosity,
};

use anyhow::Result;
use blake2::digest::{Update as _, VariableOutput as _};
use colored::Colorize;
use contract_metadata::{
    CodeHash, Compiler, Contract, ContractMetadata, Language, Source, SourceCompiler,
    SourceLanguage, SourceWasm, User,
};
use semver::Version;
use std::{fs, path::PathBuf};
use url::Url;

const METADATA_FILE: &str = "metadata.json";

/// Executes the metadata generation process
struct GenerateMetadataCommand {
    crate_metadata: CrateMetadata,
    verbosity: Option<Verbosity>,
    build_artifact: BuildArtifacts,
    unstable_options: UnstableFlags,
    debug: bool,
}

/// Result of generating the extended contract project metadata
struct ExtendedMetadataResult {
    dest_wasm: Option<PathBuf>,
    maybe_dest_debug_wasm: Option<PathBuf>,
    source: Source,
    contract: Contract,
    user: Option<User>,
    optimization_result: Option<OptimizationResult>,
}

impl GenerateMetadataCommand {
    pub fn exec(&self) -> Result<BuildResult> {
        util::assert_channel()?;

        let target_directory = self.crate_metadata.target_directory.clone();
        let out_path_metadata = target_directory.join(METADATA_FILE);

        let fname_bundle = format!("{}.contract", self.crate_metadata.package_name);
        let out_path_bundle = target_directory.join(fname_bundle);

        // build the extended contract project metadata
        let ExtendedMetadataResult {
            dest_wasm,
            maybe_dest_debug_wasm,
            source,
            contract,
            user,
            optimization_result,
        } = self.extended_metadata()?;

        let generate_metadata = |manifest_path: &ManifestPath| -> Result<()> {
            let mut current_progress = 4;
            println!(
                " {} {}",
                format!("[{}/{}]", current_progress, self.build_artifact.steps()).bold(),
                "Generating metadata".bright_green().bold()
            );
            let target_dir_arg = format!("--target-dir={}", target_directory.to_string_lossy());
            let stdout = util::invoke_cargo(
                "run",
                &[
                    "--package",
                    "metadata-gen",
                    &manifest_path.cargo_arg(),
                    &target_dir_arg,
                    "--release",
                ],
                self.crate_metadata.manifest_path.directory(),
                self.verbosity,
            )?;

            let ink_meta: serde_json::Map<String, serde_json::Value> =
                serde_json::from_slice(&stdout)?;
            let metadata = ContractMetadata::new(source, contract, user, ink_meta);
            {
                let mut metadata = metadata.clone();
                metadata.remove_source_wasm_attribute();
                let contents = serde_json::to_string_pretty(&metadata)?;
                fs::write(&out_path_metadata, contents)?;
                current_progress += 1;
            }

            if self.build_artifact == BuildArtifacts::All {
                println!(
                    " {} {}",
                    format!("[{}/{}]", current_progress, self.build_artifact.steps()).bold(),
                    "Generating bundle".bright_green().bold()
                );
                let contents = serde_json::to_string(&metadata)?;
                fs::write(&out_path_bundle, contents)?;
            }

            Ok(())
        };

        if self.unstable_options.original_manifest {
            generate_metadata(&self.crate_metadata.manifest_path)?;
        } else {
            Workspace::new(
                &self.crate_metadata.cargo_meta,
                &self.crate_metadata.root_package.id,
            )?
            .with_root_package_manifest(|manifest| {
                manifest
                    .with_added_crate_type("rlib")?
                    .with_profile_release_lto(false)?;
                Ok(())
            })?
            .with_metadata_gen_package()?
            .using_temp(generate_metadata)?;
        }

        let dest_bundle = if self.build_artifact == BuildArtifacts::All {
            Some(out_path_bundle)
        } else {
            None
        };
        Ok(BuildResult {
            dest_metadata: Some(out_path_metadata),
            dest_wasm,
            maybe_dest_debug_wasm,
            dest_bundle,
            optimization_result,
            target_directory,
            build_artifact: self.build_artifact,
        })
    }

    /// Generate the extended contract project metadata
    fn extended_metadata(&self) -> Result<ExtendedMetadataResult> {
        let contract_package = &self.crate_metadata.root_package;
        let ink_version = &self.crate_metadata.ink_version;
        let rust_version = Version::parse(&rustc_version::version()?.to_string())?;
        let contract_name = contract_package.name.clone();
        let contract_version = Version::parse(&contract_package.version.to_string())?;
        let contract_authors = contract_package.authors.clone();
        // optional
        let description = contract_package.description.clone();
        let documentation = self.crate_metadata.documentation.clone();
        let repository = contract_package
            .repository
            .as_ref()
            .map(|repo| Url::parse(&repo))
            .transpose()?;
        let homepage = self.crate_metadata.homepage.clone();
        let license = contract_package.license.clone();
        let (dest_wasm, maybe_dest_debug_wasm, hash, optimization_result) = self.wasm_hash()?;
        let source = {
            let lang = SourceLanguage::new(Language::Ink, ink_version.clone());
            let compiler = SourceCompiler::new(Compiler::RustC, rust_version);
            let maybe_wasm = if self.build_artifact == BuildArtifacts::All {
                let wasm = fs::read(&self.crate_metadata.dest_wasm)?;
                // The Wasm which we read must have the same hash as `source.hash`
                debug_assert!({
                    let expected = blake2_hash(wasm.as_slice());
                    expected == hash
                });
                Some(SourceWasm::new(wasm))
            } else {
                None
            };
            Source::new(maybe_wasm, hash, lang, compiler)
        };

        // Required contract fields
        let mut builder = Contract::builder();
        builder
            .name(contract_name)
            .version(contract_version)
            .authors(contract_authors);

        if let Some(description) = description {
            builder.description(description);
        }

        if let Some(documentation) = documentation {
            builder.documentation(documentation);
        }

        if let Some(repository) = repository {
            builder.repository(repository);
        }

        if let Some(homepage) = homepage {
            builder.homepage(homepage);
        }

        if let Some(license) = license {
            builder.license(license);
        }

        let contract = builder
            .build()
            .map_err(|err| anyhow::anyhow!("Invalid contract metadata builder state: {}", err))?;

        // user defined metadata
        let user = self.crate_metadata.user.clone().map(User::new);

        Ok(ExtendedMetadataResult {
            dest_wasm: Some(dest_wasm),
            maybe_dest_debug_wasm,
            source,
            contract,
            user,
            optimization_result: Some(optimization_result),
        })
    }

    /// Compile the contract and then hash the resulting Wasm.
    ///
    /// Return a tuple of `(dest_wasm, hash, optimization_result)`.
    fn wasm_hash(&self) -> Result<(PathBuf, Option<PathBuf>, CodeHash, OptimizationResult)> {
        let (maybe_dest_wasm, maybe_dest_debug_wasm, maybe_optimization_res) =
            super::build::execute_with_crate_metadata(
                &self.crate_metadata,
                self.verbosity,
                true, // for the hash we always use the optimized version of the contract
                self.build_artifact,
                self.unstable_options.clone(),
                self.debug,
            )?;

        let wasm = fs::read(&self.crate_metadata.dest_wasm)?;
        let dest_wasm = maybe_dest_wasm.expect("dest wasm must exist");
        let optimization_res = maybe_optimization_res.expect("optimization result must exist");
        Ok((
            dest_wasm,
            maybe_dest_debug_wasm,
            blake2_hash(wasm.as_slice()),
            optimization_res,
        ))
    }
}

/// Returns the blake2 hash of the submitted slice.
fn blake2_hash(code: &[u8]) -> CodeHash {
    let mut output = [0u8; 32];
    let mut blake2 = blake2::VarBlake2b::new_keyed(&[], 32);
    blake2.update(code);
    blake2.finalize_variable(|result| output.copy_from_slice(result));
    CodeHash(output)
}

/// Generates a file with metadata describing the ABI of the smart-contract.
///
/// It does so by generating and invoking a temporary workspace member.
pub(crate) fn execute(
    manifest_path: &ManifestPath,
    verbosity: Option<Verbosity>,
    build_artifact: BuildArtifacts,
    unstable_options: UnstableFlags,
    debug: bool,
) -> Result<BuildResult> {
    let crate_metadata = CrateMetadata::collect(manifest_path)?;
    let res = GenerateMetadataCommand {
        crate_metadata,
        verbosity,
        build_artifact,
        unstable_options,
        debug,
    }
    .exec()?;
    Ok(res)
}

#[cfg(feature = "test-ci-only")]
#[cfg(test)]
mod tests {
    use crate::cmd::metadata::blake2_hash;
    use crate::{
        cmd, crate_metadata::CrateMetadata, util::tests::with_tmp_dir, BuildArtifacts,
        ManifestPath, UnstableFlags,
    };
    use contract_metadata::*;
    use serde_json::{Map, Value};
    use std::{fmt::Write, fs};
    use toml::value;

    struct TestContractManifest {
        toml: value::Table,
        manifest_path: ManifestPath,
    }

    impl TestContractManifest {
        fn new(manifest_path: ManifestPath) -> anyhow::Result<Self> {
            Ok(Self {
                toml: toml::from_slice(&fs::read(&manifest_path)?)?,
                manifest_path,
            })
        }

        fn package_mut(&mut self) -> anyhow::Result<&mut value::Table> {
            self.toml
                .get_mut("package")
                .ok_or(anyhow::anyhow!("package section not found"))?
                .as_table_mut()
                .ok_or(anyhow::anyhow!("package section should be a table"))
        }

        /// Add a key/value to the `[package.metadata.contract.user]` section
        fn add_user_metadata_value(
            &mut self,
            key: &'static str,
            value: value::Value,
        ) -> anyhow::Result<()> {
            self.package_mut()?
                .entry("metadata")
                .or_insert(value::Value::Table(Default::default()))
                .as_table_mut()
                .ok_or(anyhow::anyhow!("metadata section should be a table"))?
                .entry("contract")
                .or_insert(value::Value::Table(Default::default()))
                .as_table_mut()
                .ok_or(anyhow::anyhow!(
                    "metadata.contract section should be a table"
                ))?
                .entry("user")
                .or_insert(value::Value::Table(Default::default()))
                .as_table_mut()
                .ok_or(anyhow::anyhow!(
                    "metadata.contract.user section should be a table"
                ))?
                .insert(key.into(), value);
            Ok(())
        }

        fn add_package_value(
            &mut self,
            key: &'static str,
            value: value::Value,
        ) -> anyhow::Result<()> {
            self.package_mut()?.insert(key.into(), value);
            Ok(())
        }

        fn write(&self) -> anyhow::Result<()> {
            let toml = toml::to_string(&self.toml)?;
            fs::write(&self.manifest_path, toml).map_err(Into::into)
        }
    }

    #[test]
    fn generate_metadata() {
        env_logger::try_init().ok();
        with_tmp_dir(|path| {
            cmd::new::execute("new_project", Some(path)).expect("new project creation failed");
            let working_dir = path.join("new_project");
            let manifest_path = ManifestPath::new(working_dir.join("Cargo.toml"))?;

            // add optional metadata fields
            let mut test_manifest = TestContractManifest::new(manifest_path)?;
            test_manifest.add_package_value("description", "contract description".into())?;
            test_manifest.add_package_value("documentation", "http://documentation.com".into())?;
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

            let crate_metadata = CrateMetadata::collect(&test_manifest.manifest_path)?;
            let dest_bundle = cmd::metadata::execute(
                &test_manifest.manifest_path,
                None,
                BuildArtifacts::All,
                UnstableFlags::default(),
            )?
            .dest_bundle
            .expect("bundle file not found");
            let metadata_json: Map<String, Value> =
                serde_json::from_slice(&fs::read(&dest_bundle)?)?;

            assert!(
                dest_bundle.exists(),
                format!("Missing metadata file '{}'", dest_bundle.display())
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
            let expected_hash = blake2_hash(&fs_wasm[..]);
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
                serde_json::Value::Array(
                    vec!["and".into(), "their".into(), "values".into()].into(),
                ),
            );

            assert_eq!(build_byte_str(&expected_hash.0[..]), hash.as_str().unwrap());
            assert_eq!(expected_wasm, wasm.as_str().unwrap());
            assert_eq!(expected_language, language.as_str().unwrap());
            assert_eq!(expected_compiler, compiler.as_str().unwrap());
            assert_eq!(crate_metadata.package_name, name.as_str().unwrap());
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
        })
    }

    fn build_byte_str(bytes: &[u8]) -> String {
        let mut str = String::new();
        write!(str, "0x").expect("failed writing to string");
        for byte in bytes {
            write!(str, "{:02x}", byte).expect("failed writing to string");
        }
        str
    }
}

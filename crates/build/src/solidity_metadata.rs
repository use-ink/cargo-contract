// Copyright (C) ink! contributors.
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

mod abi;
mod natspec;

use std::{
    collections::HashMap,
    fs::{
        self,
        File,
    },
    path::{
        Path,
        PathBuf,
    },
};

use alloy_json_abi::JsonAbi;
use anyhow::{
    Context,
    Result,
};
use cargo_metadata::TargetKind;
use contract_metadata::{
    CodeHash,
    Contract,
    Source,
};
use ink_metadata::InkProject;
use serde::{
    de,
    ser::SerializeMap,
    Deserialize,
    Serialize,
};

use self::natspec::{
    DevDoc,
    UserDoc,
};
use crate::{
    code_hash,
    CrateMetadata,
};

// Re-exports abi utilities.
pub use self::abi::{
    abi_path,
    generate_abi,
    write_abi,
};

/// Solidity compatible smart contract metadata.
///
/// Ref: <https://docs.soliditylang.org/en/latest/metadata.html>
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SolidityContractMetadata {
    /// Details about the compiler.
    compiler: Compiler,
    /// Source code language
    language: String,
    /// Generated information about the contract.
    pub output: Output,
    /// Compiler settings.
    /// Required by the spec, but very Solidity/EVM specific.
    // TODO: (@davidsemakula) include ink! compiler settings instead?
    #[serde(
        serialize_with = "serialize_to_empty_map",
        deserialize_with = "deserialize_to_unit"
    )]
    settings: (),
    /// Compilation source files/source units, keys are file paths.
    sources: HashMap<String, SourceFile>,
    /// The version of the metadata format.
    version: u8,
}

/// Details about the compiler.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Compiler {
    /// Hash of the compiler binary which produced this output.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "keccak256")]
    hash: Option<CodeHash>,
    /// Version of the compiler.
    version: String,
}

/// Generated information about the contract.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Output {
    /// ABI definition of the contract.
    /// Ref: <https://docs.soliditylang.org/en/latest/abi-spec.html#json>
    pub abi: JsonAbi,
    /// NatSpec developer documentation of the contract.
    /// Ref: <https://docs.soliditylang.org/en/latest/natspec-format.html#developer-documentation>
    #[serde(rename = "devdoc")]
    dev_doc: DevDoc,
    /// NatSpec user documentation of the contract.
    /// Ref: <https://docs.soliditylang.org/en/latest/natspec-format.html#user-documentation>
    #[serde(rename = "userdoc")]
    user_doc: UserDoc,
}

/// Compilation source files/source units, keys are file paths.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct SourceFile {
    /// Contents of the source file.
    content: String,
    /// Hash of the source file.
    #[serde(rename = "keccak256")]
    hash: CodeHash,
    /// SPDX license identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    license: Option<String>,
}

impl SourceFile {
    /// Creates a source file.
    fn new(content: String, license: Option<String>) -> Self {
        let hash = code_hash(content.as_bytes());
        Self {
            hash: CodeHash::from(hash),
            content,
            license,
        }
    }
}

/// Generates a contract metadata file compatible with the Solidity metadata specification
/// for the ink! smart contract.
///
/// Ref: <https://docs.soliditylang.org/en/latest/metadata.html>
pub fn generate_metadata(
    ink_project: &InkProject,
    abi: JsonAbi,
    source: Source,
    contract: Contract,
    crate_metadata: &CrateMetadata,
) -> Result<SolidityContractMetadata> {
    let sources = source_files(crate_metadata)?;
    let (dev_doc, user_doc) = natspec::generate_natspec(ink_project, contract)?;
    let metadata = SolidityContractMetadata {
        compiler: Compiler {
            hash: None,
            version: source.compiler.to_string(),
        },
        language: source.language.to_string(),
        output: Output {
            abi,
            dev_doc,
            user_doc,
        },
        sources,
        settings: (),
        version: 1,
    };

    Ok(metadata)
}

/// Get the path of the Solidity compatible contract metadata file.
pub fn metadata_path(crate_metadata: &CrateMetadata) -> PathBuf {
    let metadata_file = format!("{}.json", crate_metadata.contract_artifact_name);
    crate_metadata.target_directory.join(metadata_file)
}

/// Writes a Solidity compatible metadata file.
///
/// Ref: <https://docs.soliditylang.org/en/latest/metadata.html>
pub fn write_metadata<P>(metadata: &SolidityContractMetadata, path: P) -> Result<()>
where
    P: AsRef<Path>,
{
    let json = serde_json::to_string(metadata)?;
    fs::write(path, json)?;

    Ok(())
}

/// Reads the file and tries to parse it as instance of [`SolidityContractMetadata`].
pub fn load_metadata<P>(metadata_path: P) -> Result<SolidityContractMetadata>
where
    P: AsRef<Path>,
{
    let path = metadata_path.as_ref();
    let file = File::open(path)
        .context(format!("Failed to open metadata file {}", path.display()))?;
    serde_json::from_reader(file).context(format!(
        "Failed to deserialize metadata file {}",
        path.display()
    ))
}

/// Returns compilation source file content, keys are relative file paths.
fn source_files(crate_metadata: &CrateMetadata) -> Result<HashMap<String, SourceFile>> {
    let mut source_files = HashMap::new();

    // Adds `Cargo.toml` source.
    let manifest_path = &crate_metadata.manifest_path;
    let project_dir = manifest_path.absolute_directory()?;
    let manifest_path_buf = PathBuf::from(manifest_path.clone());
    let manifest_key = manifest_path_buf
        .strip_prefix(&project_dir)
        .unwrap_or_else(|_| &manifest_path_buf)
        .to_string_lossy()
        .into_owned();
    let manifest_content = fs::read_to_string(&manifest_path_buf)?;
    source_files.insert(
        manifest_key,
        SourceFile::new(
            manifest_content,
            crate_metadata.root_package.license.clone(),
        ),
    );

    // Adds `lib.rs` source.
    let lib_src_path = &crate_metadata
        .root_package
        .targets
        .iter()
        .find_map(|target| {
            (target.kind == [TargetKind::Lib]).then_some(target.src_path.clone())
        })
        .context("Couldn't find `lib.rs` path")?;
    let lib_src_content = fs::read_to_string(lib_src_path)?;
    let lib_src_key = lib_src_path
        .strip_prefix(&project_dir)
        .unwrap_or_else(|_| lib_src_path)
        .to_string();
    source_files.insert(
        lib_src_key,
        SourceFile::new(lib_src_content, crate_metadata.root_package.license.clone()),
    );

    Ok(source_files)
}

/// Serializes to an empty map (regardless of input).
pub fn serialize_to_empty_map<S>(_: &(), serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let map = serializer.serialize_map(Some(0))?;
    map.end()
}

/// Deserializes to a unit (expects and empty map or null).
pub fn deserialize_to_unit<'de, D>(deserializer: D) -> Result<(), D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct BinOpVisitor;

    impl<'de> de::Visitor<'de> for BinOpVisitor {
        type Value = ();

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(formatter, "an empty map or null")
        }

        fn visit_map<A>(self, _map: A) -> Result<Self::Value, A::Error>
        where
            A: de::MapAccess<'de>,
        {
            Ok(())
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(())
        }
    }

    deserializer.deserialize_str(BinOpVisitor)
}

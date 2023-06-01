use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use anyhow::{Error, Result};
use byte_unit::Byte;
use contract_build::{BuildResult, MetadataArtifacts};
use serde_json::{Map, Value};

const BUILD_INFO_PATH: &str = "build_info.json";

use crate::cmd::printer::print_build_info;

// Save a summary of each smart contract that is built
// to a file build_info.json in the project root
pub fn save(build_result: &BuildResult) -> Result<(), Error> {
    let mut target_directory = build_result
        .target_directory
        .as_path()
        .display()
        .to_string();
    let target_directory_short = match &target_directory.rfind("target") {
        Some(index) => target_directory.split_off(*index),
        None => "".to_string(), // unknown target directory
    };
    let metadata_artifacts: &MetadataArtifacts = match &build_result.metadata_result {
        Some(ma) => ma,
        None => anyhow::bail!("Missing metadata_result in build result"),
    };
    let metadata_json_path = metadata_artifacts
        .dest_metadata
        .as_path()
        .display()
        .to_string();
    let file_metadata = File::open(metadata_json_path)?;
    let mut buf_reader = BufReader::new(&file_metadata);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;
    let file_metadata_len = &file_metadata.metadata().unwrap().len();
    let byte = Byte::from_bytes(<u64 as Into<u128>>::into(*file_metadata_len));
    let adjusted_byte = byte.get_appropriate_unit(false);
    let file_len_units = &adjusted_byte.to_string();
    let metadata_json: Map<String, Value> =
        serde_json::from_slice::<Map<String, Value>>(&contents.as_bytes())?;
    let contract_name = metadata_json["storage"]["root"]["layout"]["struct"]["name"]
        .as_str()
        .unwrap();
    let contract_map = HashMap::from([
        ("Contract", contract_name),
        ("Size", file_len_units),
        ("Metadata Path", &target_directory_short),
    ]);
    let build_data = vec![&contract_map];
    let exists_build_info_path = Path::new(BUILD_INFO_PATH).exists();
    if !exists_build_info_path {
        // build_info.json doesn't exist, so create it with the data
        serde_json::to_writer(&File::create(BUILD_INFO_PATH)?, &build_data)?;
    } else {
        print_build_info(true, false, Some(contract_name), Some(contract_map))?;
    }
    Ok(())
}

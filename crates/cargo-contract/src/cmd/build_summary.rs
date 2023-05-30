use anyhow::{Error, Result};
use contract_build::{
    BuildResult,
    MetadataArtifacts,
};
use serde_json::{
    Map,
    Value,
};
use byte_unit::Byte;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

const BUILD_INFO_PATH: &str = "build_info.json";

// Save a summary of each smart contract that is built
// to a file build_info.json in the project root
pub fn save(build_result: &BuildResult) -> Result<(), Error> {
    // println!("build_result {:#?}", &build_result);
    let mut target_directory = build_result.target_directory
        .as_path().display().to_string();
    let target_directory_short = match &target_directory.rfind("target") {
        Some(index) => target_directory.split_off(*index),
        None => "".to_string(), // unknown target directory
    };
    // println!("target_directory_short: {}", &target_directory_short);

    let metadata_artifacts: &MetadataArtifacts =
        match &build_result.metadata_result {
            Some(ma) => ma,
            None => anyhow::bail!("Missing metadata_result in build result"),
        };
    let metadata_json_path = metadata_artifacts.dest_metadata
        .as_path().display().to_string();
    // println!("metadata_json_path {:?}", metadata_json_path);
    let file_metadata = File::open(metadata_json_path)?;
    let mut buf_reader = BufReader::new(&file_metadata);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;
    let file_metadata_len = &file_metadata.metadata().unwrap().len();
    let byte = Byte::from_bytes(<u64 as Into<u128>>::into(*file_metadata_len));
    let adjusted_byte = byte.get_appropriate_unit(false);
    let file_len_units = &adjusted_byte.to_string();
    // println!("file len in units {}", &adjusted_byte.to_string());

    let metadata_json: Map<String, Value> =
        serde_json::from_slice::<Map<String, Value>>(&contents.as_bytes())?;
    let contract_name = metadata_json["storage"]["root"]["layout"]["struct"]["name"].as_str().unwrap();
    // println!("contract_name {:?}", &contract_name);
    // println!("metadata_json {:?}", metadata_json);
    let contract_map = HashMap::from([
        ("Contract", contract_name),
        ("Size", file_len_units),
        ("Metadata Path", &target_directory_short),
    ]);
    let build_data = vec![
        &contract_map
    ];
    // println!("contract_map {:#?}", contract_map.clone());
    // println!("build_data {:#?}", &build_data);

    let exists_build_info_path = Path::new(BUILD_INFO_PATH).exists();
    if !exists_build_info_path {
        // println!("not existing path");
        // build_info.json doesn't exist, so create it with the data
        serde_json::to_writer(&File::create(BUILD_INFO_PATH)?, &build_data)?;
    } else {
        print_build_info(true, Some(contract_name), Some(contract_map))?;
    }
    Ok(())
}

// Print a summary of all the smart contracts that have been built
// that have been included in a file build_info.json in the project root.
// If it is called whilst contract is being built then `write_new_build`
// will be `true` and that contract will be added to or updated in build_info.json
pub fn print_build_info(write_new_build: bool,
    contract_name: Option<&str>, contract_map: Option<HashMap<&str, &str>>) -> Result<(), Error> {
    let exists_build_info_path = Path::new(BUILD_INFO_PATH).exists();
    if !exists_build_info_path {
        // println!("not existing path");
        anyhow::bail!("Unable to print from or write to file that does not exist");
    }
    // println!("existing path");
    // build_info.json exists, so update it with the data
    let file_build_info = File::open(BUILD_INFO_PATH)?;
    let mut buf_reader = BufReader::new(&file_build_info);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;
    let mut build_info_json: Vec<HashMap<&str, &str>> =
        serde_json::from_slice::<Vec<HashMap<&str, &str>>>(&contents.as_bytes())?;
    // println!("build_info_json {:#?}", build_info_json);

    if write_new_build == true {
        let mut found = false;
        for info in build_info_json.iter_mut() {
            // replace existing build info with new contract info
            // if the contract name already exists as a value in build_info.json
            let c = match info.get(&"Contract") {
                Some(c) => c,
                None => "",
            };
            if c == contract_name.unwrap() {
                found = true;
                info.insert("Size", match contract_map.clone().unwrap().get(&"Size") {
                    Some(s) => s,
                    None => "",
                });
                info.insert("Metadata Path", match contract_map.clone().unwrap().get(&"Metadata Path") {
                    Some(m) => m,
                    None => "",
                });
            }
        }
        // if did not find an existing value in build_info_json to update
        // then push the new value to the end
        if found == false {
            build_info_json.push(contract_map.clone().unwrap());
        }
        // write updated to file
        serde_json::to_writer(&File::create("build_info.json")?,
        &build_info_json.clone())?;
    }
    if self.output_json {
        println!("{:#?}", build_info_json);
    } else {
        // TODO - replace with tabular format when they don't use
        // `--output-build-info-json`
        println!("{:#?}", build_info_json);
    }

    Ok(())
}

#[derive(Debug, clap::Args)]
#[clap(name = "summary", about = "Get info about built contracts")]
pub struct SummaryCommand {
    /// Export the built contracts info output in JSON format.
    #[clap(name = "output-build-info-json", long)]
    build_info_json: bool,
}

impl SummaryCommand {
    pub fn run(&self) -> Result<(), Error> {
        print_build_info(false, None, None)?;

        Ok(())
    }
}

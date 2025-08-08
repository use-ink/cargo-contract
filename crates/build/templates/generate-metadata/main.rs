extern crate contract;

use serde::{Deserialize, Serialize};

extern "Rust" {
    // Note: The ink! metadata codegen generates an implementation for these functions,
    // which is what we end up linking to here.
    fn __ink_generate_metadata() -> ink::metadata::InkProject;
    fn __ink_generate_solidity_metadata() -> ink::metadata::sol::ContractMetadata;
}

#[derive(Serialize, Deserialize)]
pub struct Metadata {
    ink: Option<ink::metadata::InkProject>,
    solidity: Option<ink::metadata::sol::ContractMetadata>,
}

fn main() -> Result<(), std::io::Error> {
    // Generate ink! metadata if ABI is NOT "sol".
    let ink_meta = if cfg!(not(ink_abi = "sol")) {
        Some(unsafe { __ink_generate_metadata() })
    } else {
        None
    };
    // Generate Solidity ABI compatibility metadata if ABI is "sol" or "all".
    let sol_meta = if cfg!(any(ink_abi = "sol", ink_abi = "all")) {
        Some(unsafe { __ink_generate_solidity_metadata() })
    } else {
        None
    };
    let metadata = Metadata {
        ink: ink_meta,
        solidity: sol_meta,
    };
    let contents = serde_json::to_string_pretty(&metadata)?;
    print!("{contents}");
    Ok(())
}

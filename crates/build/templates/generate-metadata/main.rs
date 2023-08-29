extern crate contract;

extern "Rust" {
    // Note: The ink! metdata codegen generates an implementation for this function,
    // which is what we end up linking to here.
    fn __ink_generate_metadata() -> ink::metadata::InkProject;
}

fn main() -> Result<(), std::io::Error> {
    let metadata = unsafe { __ink_generate_metadata() };
    let contents = serde_json::to_string_pretty(&metadata)?;
    print!("{contents}");
    Ok(())
}

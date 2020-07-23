extern crate contract;

extern "Rust" {
	fn __ink_generate_metadata() -> ink_metadata::InkProject;
}

fn main() -> Result<(), std::io::Error> {
	let metadata = unsafe { __ink_generate_metadata() };
	let contents = serde_json::to_string_pretty(&metadata)?;
	print!("{}", contents);
	Ok(())
}

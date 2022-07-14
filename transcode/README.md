# Contract Transcode

Contains utilities for encoding smart contract calls to SCALE.

Currently part of [`cargo-contract`](https://github.com/paritytech/cargo-contract), the build tool for smart
 contracts written in [ink!](https://github.com/paritytech/ink).


# Example

```rust
use transcode::ContractMessageTranscoder;

fn main() {
    let metadata_path = "/path/to/metadata.json";

    let metadata = load_metadata(&metadata_path.into())?;
    let transcoder = ContractMessageTranscoder::new(&metadata);

    let constructor = "new";
    let args = ["foo", "bar"];
    let data = transcoder.encode(&constructor, &args).unwrap();

    println!("Encoded constructor data {:?}", data);
}

fn load_metadata(path: &Path) -> anyhow::Result<ink_metadata::InkProject> {
    let file = File::open(&path).expect("Failed to open metadata file");
    let metadata: ContractMetadata =
        serde_json::from_reader(file).expect("Failed to deserialize metadata file");

    let ink_metadata: ink_metadata::InkProject = serde_json::from_value(
        serde_json::Value::Object(metadata.abi),
    ).expect("Failed to deserialize ink project metadata");

    if let ink_metadata::MetadataVersion::V4 = ink_metadata.version() {
        Ok(ink_metadata)
    } else {
        Err(anyhow!("Unsupported ink metadata version. Expected V4"))
    }
}

```

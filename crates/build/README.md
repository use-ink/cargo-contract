# contract-build

A crate for building [`ink!`](https://github.com/use-ink/ink) smart contracts. Used by
[`cargo-contract`](https://github.com/use-ink/cargo-contract).

## Usage

```rust
use contract_build::{
    ManifestPath,
    Verbosity,
    BuildArtifacts,
    BuildMode,
    Features,
    MetadataSpec,
    Network,
    OutputType,
    UnstableFlags,
    Target,
    ImageVariant,
};

let manifest_path = ManifestPath::new("my-contract/Cargo.toml").unwrap();

let args = contract_build::ExecuteArgs {
    manifest_path,
    verbosity: Verbosity::Default,
    build_mode: BuildMode::Release,
    features: Features::default(),
    network: Network::Online,
    build_artifact: BuildArtifacts::All,
    unstable_flags: UnstableFlags::default(),
    keep_debug_symbols: false,
    extra_lints: false,
    output_type: OutputType::Json,
    image: ImageVariant::Default,
    metadata_spec: MetadataSpec::Ink,
};

contract_build::execute(args);
```

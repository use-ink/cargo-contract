# contract-build

A crate for building [`ink!`](https://github.com/paritytech/ink) smart contracts. Used by 
[`cargo-contract`](https://github.com/paritytech/cargo-contract).

## Usage

```Rust
let args = contract_build::ExecuteArgs {
    manifest_path,
    verbosity,
    build_mode,
    network,
    build_artifact
    unstable_flags,
    optimization_passes,
    keep_debug_symbols,
    lint,
    output_type,
};

contract_build::execute(args)
```
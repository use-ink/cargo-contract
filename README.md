# Cargo plugin for [`ink!`](https://github.com/paritytech/ink) contracts

[![GitHub license](https://img.shields.io/github/license/paritytech/cargo-contract)](LICENSE) [![GitLab Status](https://gitlab.parity.io/parity/cargo-contract/badges/master/pipeline.svg)](https://gitlab.parity.io/parity/cargo-contract/pipelines)

**IMPORTANT NOTE:** WORK IN PROGRESS! Do not expect this to be working. 

A small CLI tool for helping setting up and managing WebAssembly smart contracts written with ink!.

## Installation

`cargo install --git https://github.com/paritytech/cargo-contract cargo-contract --force`

Use the --force to ensure you are updated to the most recent cargo-contract version.

## Usage

```
cargo-contract 0.3.0
Utilities to develop Wasm smart contracts.

USAGE:
    cargo contract <SUBCOMMAND>

OPTIONS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    new                  Setup and create a new smart contract project
    build                Compiles the smart contract
    generate-metadata    Generate contract metadata artifacts
    test                 Test the smart contract off-chain
    deploy               Upload the smart contract code to the chain
    instantiate          Instantiate a deployed smart contract
    help                 Prints this message or the help of the given subcommand(s)
```

## Contract build config

Building the contract uses `cargo-xbuild` under the hood for optimum Wasm binary size. This requires the following
configuration section to be added to your contract's `Cargo.toml`:

```
[package.metadata.cargo-xbuild]
panic_immediate_abort = true
```

This will perform a custom build of Rust's `libcore` [without panic strings and formatting code](https://github.com/johnthagen/min-sized-rust#remove-panic-string-formatting-with-panic_immediate_abort), which significantly 
reduces the final binary size.

## Features

The `deploy` and `instantiate` subcommands are **disabled by default**, since they are not fully stable yet and increase the build time.

If you want to try them, you need to enable the `extrinsics` feature:

`cargo install --git https://github.com/paritytech/cargo-contract cargo-contract --features extrinsics --force`

Once they are stable and the compilation time is acceptable, we will consider removing the `extrinsics` feature.

## License

The entire code within this repository is licensed under the [GPLv3](LICENSE). Please [contact us](https://www.parity.io/contact/) if you have questions about the licensing of our products.



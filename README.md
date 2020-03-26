# Cargo plugin for [`ink!`](https://github.com/paritytech/ink) contracts

[![GitHub license](https://img.shields.io/github/license/paritytech/cargo-contract)](LICENSE) 
[![GitLab Status](https://gitlab.parity.io/parity/cargo-contract/badges/master/pipeline.svg)](https://gitlab.parity.io/parity/cargo-contract/pipelines)
[![Latest Version](https://img.shields.io/crates/v/cargo-contract.svg)](https://crates.io/crates/cargo-contract)

A CLI tool for helping setting up and managing WebAssembly smart contracts written with ink!.

## Installation

### Prerequisites

- **rust-src**: `rustup component add rust-src`
- **wasm-opt**: https://github.com/WebAssembly/binaryen#tools

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

## `build` requires the `nightly` toolchain

`cargo contract build` must be run using the `nightly` toolchain. If you have 
[`rustup`](https://github.com/rust-lang/rustup) installed, the simplest way to do so is `cargo +nightly contract build`.
To avoid having to add `+nightly` you can also create a `rust-toolchain` file in your local directory containing 
`nightly`. Read more about how to [specify the rustup toolchain](https://github.com/rust-lang/rustup#override-precedence).

## Features

The `deploy` and `instantiate` subcommands are **disabled by default**, since they are not fully stable yet and increase the build time.

If you want to try them, you need to enable the `extrinsics` feature:

`cargo install --git https://github.com/paritytech/cargo-contract cargo-contract --features extrinsics --force`

Once they are stable and the compilation time is acceptable, we will consider removing the `extrinsics` feature.

## License

The entire code within this repository is licensed under the [GPLv3](LICENSE). Please [contact us](https://www.parity.io/contact/) if you have questions about the licensing of our products.



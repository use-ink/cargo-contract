<div align="center">
    <img src="./.images/cargo-contract.svg" alt="cargo-contract" height="170" />

[![CI Status][a1]][a2]
[![Latest Release][d1]][d2]
[![stack-exchange][s1]][s2]

[a1]: https://github.com/paritytech/cargo-contract/workflows/ci/badge.svg
[a2]: https://github.com/paritytech/cargo-contract/actions?query=workflow%3Aci+branch%3Amaster
[d1]: https://img.shields.io/crates/v/cargo-contract.svg
[d2]: https://crates.io/crates/cargo-contract
[s1]: https://img.shields.io/badge/click-white.svg?logo=StackExchange&label=ink!%20Support%20on%20StackExchange&labelColor=white&color=blue
[s2]: https://substrate.stackexchange.com/questions/tagged/ink?tab=Votes

<p align="center">

> <img src="./.images/ink-squid.svg" alt="squink, the ink! mascot" style="vertical-align: middle" align="left" height="60" />`cargo-contract` is a CLI tool which helps you develop smart contracts in Parity's <a href="https://github.com/paritytech/ink">ink!</a>.<br/>ink! is a Rust [eDSL](https://wiki.haskell.org/Embedded_domain_specific_language) which allows you to write smart contracts for blockchains built on the [Substrate](https://github.com/paritytech/substrate) framework.

</p>

<br/>

[Guided Tutorial for Beginners](https://docs.substrate.io/tutorials/v3/ink-workshop/pt1/)&nbsp;&nbsp;•&nbsp;&nbsp;
[ink! Documentation Portal](https://ink.substrate.io)

<br/>
</div>

More relevant links:

-   Find answers to your questions by joining our [Stack Exchange][s2] community
-   [ink!](https://github.com/paritytech/ink) ‒ The main ink! repository with smart contract examples
-   [Contracts UI](https://contracts-ui.substrate.io/) ‒ Frontend for contract deployment and interaction
-   [Substrate Contracts Node](https://github.com/paritytech/substrate-contracts-node) ‒ Simple Substrate blockchain which includes smart contract functionality

## Installation

In addition to Rust, installation requires a C++ compiler that supports C++17.
Modern releases of gcc and clang, as well as Visual Studio 2019+ should work.

-   Step 1: `rustup component add rust-src`.

-   Step 2: `cargo install --force --locked cargo-contract`.

-   Step 3: (**Optional**) Install `dylint` for linting.

    -   (MacOS) `brew install openssl`
    -   `cargo install cargo-dylint dylint-link`.

-   Step 4: (**Optional**) Install [Docker Engine](https://docs.docker.com/engine/install)
    to be able to produce verifiable builds.

You can always update the `cargo-contract` binary to the latest version by running the Step 2.

### Installation using Docker Image

If you prefer to use Docker instead we have a Docker image
[available on the Docker Hub](https://hub.docker.com/r/paritytech/contracts-ci-linux):

```bash
# Pull the latest stable image.
docker pull paritytech/contracts-ci-linux

# Create a new contract in your current directory.
docker run --rm -it -v $(pwd):/sources paritytech/contracts-ci-linux \
  cargo contract new --target-dir /sources my_contract

# Build the contract. This will create the contract file under
# `my_contract/target/ink/my_contract.contract`.
docker run --rm -it -v $(pwd):/sources paritytech/contracts-ci-linux \
  cargo contract build --manifest-path=/sources/my_contract/Cargo.toml
```

**Windows:** If you use PowerShell, change `$(pwd)` to `${pwd}`.

```bash
# Create a new contract in your current directory (in Windows PowerShell).
docker run --rm -it -v ${pwd}:/sources paritytech/contracts-ci-linux \
  cargo contract new --target-dir /sources my_contract
```

If you want to reproduce other steps of CI process you can use the following
[guide](https://github.com/paritytech/scripts#reproduce-ci-locally).

### Verifiable builds

Some block explorers require the Wasm binary to be compiled in the deterministic environment.
To achieve it, you should build your contract using Docker image we provide:

```bash
cargo contract build --verifiable
```

You can find more detailed documentation how to use the image [here](/build-image/README.md)

## Usage

You can always use `cargo contract help` to print information on available
commands and their usage.

For each command there is also a `--help` flag with info on additional parameters,
e.g. `cargo contract new --help`.

##### `cargo contract new my_contract`

Creates an initial smart contract with some scaffolding code into a new
folder `my_contract` .

The contract contains the source code for the [`Flipper`](https://github.com/paritytech/ink-examples/blob/main/flipper/lib.rs)
contract, which is about the simplest "smart" contract you can build ‒ a `bool` which gets flipped
from `true` to `false` through the `flip()` function.

##### `cargo contract build`

Compiles the contract into optimized WebAssembly bytecode, generates metadata for it,
and bundles both together in a `<name>.contract` file, which you can use for
deploying the contract on-chain.

##### `cargo contract check`

Checks that the code builds as WebAssembly. This command does not output any `<name>.contract`
artifact to the `target/` directory.

##### `cargo contract upload`

Upload a contract to a `pallet-contracts` enabled chain. See [extrinsics](crates/extrinsics/README.md).

##### `cargo contract instantiate`

Create an instance of a contract on chain. See [extrinsics](crates/extrinsics/README.md).

##### `cargo contract call`

Invoke a message on an existing contract on chain. See [extrinsics](crates/extrinsics/README.md).

##### `cargo contract decode`

Decodes a contracts input or output data.

This can be either an event, an invocation of a contract message, or an invocation of a contract constructor.

The argument has to be given as hex-encoding, starting with `0x`.

##### `cargo contract remove`

Remove a contract from a `pallet-contracts` enabled chain. See [extrinsics](crates/extrinsics/README.md).

##### `cargo contract info`

Fetch and display contract information of a contract on chain. See [info](docs/info.md).

## Publishing

In order to publish a new version of `cargo-contract`:

-   Bump all crate versions, we move them in lockstep.
-   Make sure your PR is approved by one or more core developers.
-   Publish `metadata` ➜ `transcode` ➜ `extrinsics` ➜ `build` ➜ `cargo-contract`.
-   Merge you PR and push a tag `vX.X` with your version number.
-   Create a GitHub release with the changelog entries.

## License

The entire code within this repository is licensed under the [GPLv3](LICENSE).

Please [contact us](https://www.parity.io/contact/) if you have questions about
the licensing of our products.

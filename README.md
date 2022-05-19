<div align="center">
    <img src="./.images/cargo-contract.svg" alt="cargo-contract" height="170" />

[![CI Status][a1]][a2]
[![Latest Release][d1]][d2]
[![stack-exchange][s1]][s2]

[a1]: https://gitlab.parity.io/parity/mirrors/cargo-contract/badges/master/pipeline.svg
[a2]: https://gitlab.parity.io/parity/mirrors/cargo-contract/pipelines
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
* Find answers to your questions by joining our [Stack Exchange][s2] community
* [`ink!`](https://github.com/paritytech/ink) ‒ The main ink! repository with smart contract examples
* [Contracts UI](https://paritytech.github.io/contracts-ui/) ‒ Frontend for contract deployment and interaction
* [Substrate Contracts Node](https://github.com/paritytech/substrate-contracts-node) ‒ Simple Substrate blockchain which includes smart contract functionality


## Installation

* Step 1: `rustup component add rust-src`.

* Step 2: Install `binaryen` in a version >= 99:

  * [Debian/Ubuntu](https://tracker.debian.org/pkg/binaryen): `apt-get install binaryen`
  * [Homebrew](https://formulae.brew.sh/formula/binaryen): `brew install binaryen`
  * [Arch Linux](https://archlinux.org/packages/community/x86_64/binaryen/): `pacman -S binaryen`
  * Windows: [binary releases are available](https://github.com/WebAssembly/binaryen/releases)

  There's only an old version in your distributions package manager? Just use a
  [binary release](https://github.com/WebAssembly/binaryen/releases).

* Step 3: Install `dylint`
  * (MacOS) `brew install openssl`
  * `cargo install cargo-dylint dylint-link`.

* Step 4: `cargo install --force --locked cargo-contract`.

You can always update the `cargo-contract` binary to the latest version by running the Step 4.

### Installation using Docker Image

If you prefer to use Docker instead we have a Docker image
[available on the Docker Hub](https://hub.docker.com/r/paritytech/contracts-ci-linux):

```bash
# Pull the latest stable image.
docker pull paritytech/contracts-ci-linux:production

# Create a new contract in your current directory.
docker run --rm -it -v $(pwd):/sources paritytech/contracts-ci-linux:production \
  cargo +nightly contract new --target-dir /sources my_contract

# Build the contract. This will create the contract file under
# `my_contract/target/ink/my_contract.contract`.
docker run --rm -it -v $(pwd):/sources paritytech/contracts-ci-linux:production \
  cargo +nightly contract build --manifest-path=/sources/my_contract/Cargo.toml
```

If you want to reproduce other steps of CI process you can use the following
[guide](https://github.com/paritytech/scripts#reproduce-ci-locally).

## Usage

You can always use `cargo contract help` to print information on available
commands and their usage.

For each command there is also a `--help` flag with info on additional parameters,
e.g. `cargo contract new --help`.

##### `cargo contract new my_contract`

Creates an initial smart contract with some scaffolding code into a new
folder `my_contract` .

The contract contains the source code for the [`Flipper`](https://github.com/paritytech/ink/blob/master/examples/flipper/lib.rs)
contract, which is about the simplest "smart" contract you can build ‒ a `bool` which gets flipped
from `true` to `false` through the `flip()` function.

##### `cargo +nightly contract build`

Compiles the contract into optimized WebAssembly bytecode, generates metadata for it,
and bundles both together in a `<name>.contract` file, which you can use for
deploying the contract on-chain.

`cargo contract build` must be run using the `nightly` toolchain. If you have
[`rustup`](https://github.com/rust-lang/rustup) installed, the simplest way to
do so is `cargo +nightly contract build`.

To avoid having to always add `+nightly` you can also set `nightly` as the default
toolchain of a directory by executing `rustup override set nightly` in it.

##### `cargo contract check`

Checks that the code builds as WebAssembly. This command does not output any `<name>.contract`
artifact to the `target/` directory.

##### `cargo contract test`

Runs test suites defined for a smart contract off-chain.

##### `cargo contract upload`

Upload a contract to a `pallet-contracts` enabled chain. See [extrinsics](docs/extrinsics.md).

##### `cargo contract instantiate`

Create an instance of a contract on chain. See [extrinsics](docs/extrinsics.md).

##### `cargo contract call`

Invoke a message on an existing contract on chain. See [extrinsics](docs/extrinsics.md).

##### `cargo contract decode`

Decodes a contracts input or output data.

This can be either an event, an invocation of a contract message, or an invocation of a contract constructor.

The argument has to be given as hex-encoding, starting with `0x`.

## License

The entire code within this repository is licensed under the [GPLv3](LICENSE).

Please [contact us](https://www.parity.io/contact/) if you have questions about
the licensing of our products.

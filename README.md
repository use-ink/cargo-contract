<div align="center">
    <img src="./.images/cargo-contract.svg" alt="cargo-contract" height="170" />

[![CI Status][a1]][a2]
[![Matrix Chat][b1]][b2]
[![Discord Chat][c1]][c2]
[![Latest Release][d1]][d2]

[a1]: https://gitlab.parity.io/parity/cargo-contract/badges/master/pipeline.svg
[a2]: https://gitlab.parity.io/parity/cargo-contract/pipelines
[b1]: https://img.shields.io/badge/matrix-chat-brightgreen.svg?style=flat
[b2]: https://riot.im/app/#/room/#ink:matrix.parity.io
[c1]: https://img.shields.io/discord/722223075629727774?style=flat-square&label=discord
[c2]: https://discord.gg/ztCASQE
[d1]: https://img.shields.io/crates/v/cargo-contract.svg
[d2]: https://crates.io/crates/cargo-contract

<p align="center">

> <img src="./.images/ink-squid.svg" alt="squink, the ink! mascot" style="vertical-align: middle" align="left" height="60" />`cargo-contract` is a CLI tool which helps you develop smart contracts in Parity's <a href="https://github.com/paritytech/ink">ink!</a>.<br/>ink! is a Rust [eDSL](https://wiki.haskell.org/Embedded_domain_specific_language) which allows you to write smart contracts for blockchains built on the [Substrate](https://github.com/paritytech/substrate) framework.
</p>

<br/>

[Guided Tutorial for Beginners](https://substrate.dev/substrate-contracts-workshop/#/0/building-your-contract)&nbsp;&nbsp;•&nbsp;&nbsp; 
[ink! Documentation Portal](https://paritytech.github.io/ink-docs)

<br/>
</div>

More relevant links:
* Talk to us on [Element][b2] or [Discord][c2]
* [`ink!`](https://github.com/paritytech/ink) ‒ The main ink! repository with smart contract examples
* [Canvas UI](https://paritytech.github.io/canvas-ui/#/upload) ‒ Frontend for contract deployment and interaction
* [Canvas Node](https://github.com/paritytech/canvas-node) ‒ Simple Substrate blockchain which includes smart contract functionality


## Installation

* Step 1: `rustup component add rust-src`.

* Step 2: Install `binaryen` in a version >= 99:

  * [Debian/Ubuntu](https://tracker.debian.org/pkg/binaryen): `apt-get install binaryen`
  * [Homebrew](https://formulae.brew.sh/formula/binaryen): `brew install binaryen`
  * [Arch Linux](https://archlinux.org/packages/community/x86_64/binaryen/): `pacman -S binaryen`
  * Windows: [binary releases are available](https://github.com/WebAssembly/binaryen/releases)

  There's only an old version in your distributions package manager? Just use a 
  [binary release](https://github.com/WebAssembly/binaryen/releases).

* Step 3: `cargo install --force cargo-contract`

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



<details><summary>If you want to reproduce different steps of CI process you can use the following guide.</summary>
<p>

### Preparation

[Install `podman`](https://podman.io/getting-started/installation) (it's rootless) or rename it to `docker` in the following snippets (if you are OS X user install docker, podman doesn't work correctly with mounted volumes):


<details><summary>.bashrc</summary>
<p>

```bash
# "Cargo as a virtual environment in the current dir"
function cargoenvhere {
  dirname="$(basename $(pwd))"
  user=$(whoami)
  echo "Cargo as a virtual environment in" "$dirname" "dir"
  mkdir -p /home/"$user"/cache/"$dirname"
  podman run --rm -it -w /shellhere/"$dirname" -v "$(pwd)":/shellhere/"$dirname" -v /home/"$user"/cache/"$dirname"/:/cache/ -e CARGO_HOME=/cache/cargo/ -e SCCACHE_DIR=/cache/sccache/ -e CARGO_TARGET_DIR=/cache/target/ "$@"
}

# example use
# cargoenvhere paritytech/ci-linux:production /bin/bash -c 'RUSTFLAGS="-Cdebug-assertions=y -Dwarnings" RUST_BACKTRACE=1 time cargo test --workspace --locked --release --verbose --features runtime-benchmarks --manifest-path bin/node/cli/Cargo.toml'
```
</p>
</details>

<details><summary>cargoenvhere.sh</summary>
<p>

```bash
#!/bin/bash

# "Cargo as a virtual environment in the current dir"
dirname="$(basename $(pwd))"
user=$(whoami)
echo "Cargo as a virtual environment in" "$dirname" "dir"
mkdir -p /home/"$user"/cache/"$dirname"
podman run --rm -it -w /shellhere/"$dirname" -v "$(pwd)":/shellhere/"$dirname" -v /home/"$user"/cache/"$dirname"/:/cache/ -e CARGO_HOME=/cache/cargo/ -e SCCACHE_DIR=/cache/sccache/ -e CARGO_TARGET_DIR=/cache/target/ "$@"

# example use
# cargoenvhere paritytech/ci-linux:production /bin/bash -c 'RUSTFLAGS="-Cdebug-assertions=y -Dwarnings" RUST_BACKTRACE=1 time cargo test --workspace --locked --release --verbose --features runtime-benchmarks --manifest-path bin/node/cli/Cargo.toml'
end
```
</p>
</details>

<details><summary>cargoenvhere.fish</summary>
<p>

```bash
function cargoenvhere -d "Cargo as a virtual environment in the current dir"
  set dirname (basename (pwd))
  set user (whoami)
  echo "Cargo as a virtual environment in" $dirname "dir"
  mkdir -p /home/$user/cache/$dirname
  podman run --rm -it -w /shellhere/$dirname -v (pwd):/shellhere/$dirname -v /home/$user/cache/$dirname/:/cache/ -e CARGO_HOME=/cache/cargo/ -e SCCACHE_DIR=/cache/sccache/ -e CARGO_TARGET_DIR=/cache/target/ $argv
end
```
</p>
</details>

If you use OS X consider using following snippet:
<details><summary>.zshrc</summary>
<p>

```bash
function cargoenvhere {
  dirname="$(basename $(pwd))"
  echo "Cargo as a virtual environment in" "$dirname" "dir"
  docker volume inspect cargo-cache > /dev/null || docker volume create cargo-cache
  docker run --rm -it -w /shellhere/"$dirname" \
                    -v "$(pwd)":/shellhere/"$dirname" \
                    -v cargo-cache:/cache/ \
                    -e CARGO_HOME=/cache/cargo/ \
                    -e SCCACHE_DIR=/cache/sccache/ "$@"
}
```
</p>
</details>

TLDR; the function runs the named container in the current dir with  
      - redirecting the current directory into the image  
      - keeping your shell history on your host  
      - keeping Rust caches on your host, so you build faster the next time  
    example use:  

```bash
cargoenvhere paritytech/ci-linux:production /bin/bash -c 'RUSTFLAGS="-Cdebug-assertions=y -Dwarnings" RUST_BACKTRACE=1 \
time cargo test --workspace --locked --release --verbose --features runtime-benchmarks --manifest-path bin/node/cli/Cargo.toml'
```

### Execution

1.  open the CI config file (`.gitlab-ci.yml`)
2.  note `CI_IMAGE:` variable value there (for current repo it is `paritytech/contracts-ci-linux:production`)
3.  look for the job you want to reproduce and see if `*docker-env` is mentioned there (then you should use this one)
4.  note global and in-job `variables:`, in order to reproduce the job closely you might want to run it with the same `RUSTFLAGS` and `CARGO_INCREMENTAL`
5.  `podman pull [CI image name]` / `docker pull [CI image name]`
6.  execute your job how it's shown in the example ^ `cargoenvhere [CI image name] /bin/bash -c ‘[cargo build ...]’`
7.  find your artifacts in `/home/$USER/cache/[project name or current dir name]/target` for Linux users or `/path/to/the/cloned/repo/target` for OS X users.

:warning: If you want to execute a binary on OS X pay attention that with docker it is compiled for Linux. So if want to run it you need to use something like: `cargoenvhere paritytech/contracts-ci-linux:production cargo run`

</p>
</details>

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

## License

The entire code within this repository is licensed under the [GPLv3](LICENSE).

Please [contact us](https://www.parity.io/contact/) if you have questions about
the licensing of our products.

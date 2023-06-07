# Verifiable build using Docker

Docker image based on our base CI image `<base-ci-linux:latest>`.

Used for reproducible builds in `cargo contract --verifiable`

## Dependencies and Tools

- `llvm-dev`
- `zlib1g-dev`
- `npm`
- `yarn`
- `wabt`
- `binaryen`

**Inherited from `<base-ci-linux:latest>`**

- `libssl-dev`
- `clang-10`
- `lld`
- `libclang-dev`
- `make`
- `cmake`
- `git`
- `pkg-config`
- `curl`
- `time`
- `rhash`
- `ca-certificates`
- `jq`

**Rust versions:**

Currently, the 1.69 toolchain is temporarily required to build ink! contracts because of https://github.com/paritytech/cargo-contract/issues/1139

**Rust tools & toolchains:**

We use stable releases from crates.io

- `cargo-contract`
- `cargo-dylint` and `dylint-link`
- `pwasm-utils-cli`
- `solang`
- `wasm32-unknown-unknown`: The toolchain to compile Rust codebases for Wasm.

[Click here](https://hub.docker.com/repository/docker/paritytech/contracts-ci-linux) for the registry.

## Usage

The default entry point is `ENTRYPOINT [ "cargo", "contract", "build", "--release" ]`
and work directory is set to `/contract`. You need to mount the folder with the contract to the container.

```bash
docker run -d \
    --name ink-container \
    --mount type=bind,source="$(pwd)",target="/contract" \
    paritytech/contracts-verified
```

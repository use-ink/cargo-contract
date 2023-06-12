# Verifiable builds using Docker

Docker image based on our base CI image `<base-ci-linux:latest>`.

Used for reproducible builds in `cargo contract build --verifiable`

**Rust versions:**

Currently, the 1.69 toolchain is temporarily required to build ink! contracts because of https://github.com/paritytech/cargo-contract/issues/1139

**Rust tools & toolchains:**

We use stable releases from crates.io

- `cargo-contract`
- `wasm32-unknown-unknown`: The toolchain to compile Rust codebases for Wasm.

[Click here](https://hub.docker.com/repository/docker/paritytech/contracts-verifiable) for the registry.

## Usage

The default entry point is `ENTRYPOINT [ "cargo", "contract", "build", "--release" ]`
and work directory is set to `/contract`. You need to mount the folder with the contract to the container.

```bash
docker run -d \
    --name ink-container \
    --mount type=bind,source="$(pwd)",target="/contract" \
    paritytech/contracts-verified
```

For multi-contract projects, like in the example below:
```
my-app/
├─ ink-project-a/
│  ├─ Cargo.toml
│  ├─ lib.rs
├─ ink-project-b/
│  ├─ Cargo.toml
│  ├─ lib.rs
├─ rust-toolchain
```
Make sure to run the command inside `my-app` directory and specify relative manifest paths:
`cargo contract build --verifiable --release --manifest-path ink-project-a/Cargo.toml`

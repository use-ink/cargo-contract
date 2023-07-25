# Verifiable builds using Docker

Docker image based on the minimalistic Debian image `bitnami/minideb:bullseye-amd64`.

Used for reproducible builds in `cargo contract build --verifiable`

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
Make sure to run the command inside `my-app` directory and specify a relative manifest path
to the root contract:
`cargo contract build --verifiable --release --manifest-path ink-project-a/Cargo.toml`


**Apple Silicon performance**

It is a known issue that running AMD64 image on the ARM host architecture significantly impacts the performance
and build times. To solve this issues, enable _Use Rosetta for x86/amd64 emulation on Apple Silicon_ in
_Settings_ -> _Features in development_ tab in Docker Desktop App.

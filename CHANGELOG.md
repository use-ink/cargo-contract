# Version 0.6.0 (2020-03-25)

- First release to crates.io
- Use `subxt` release from [crates.io](https://crates.io/crates/substrate-subxt)

# Version 0.5.0 (2020-03-18)

- Upgrades dependencies [#45](https://github.com/paritytech/cargo-contract/pull/45)
- Update template to ink! 2.0 dependencies [#47](https://github.com/paritytech/cargo-contract/pull/47)

# Version 0.4.1 (2020-02-26)

- Fix: fail the whole build process if the contract build fails.

# Version 0.4.0 (2020-02-26)

- Minimize contract wasm binary size:
  - Run `wasm-opt` on the contract Wasm binary.
  - Uses [`cargo-xbuild`](https://github.com/rust-osdev/cargo-xbuild) to build custom sysroot crates without panic string
bloat.
  - Automatically removes the `rlib` crate type from `Cargo.toml`, removing redundant metadata.
- Removes requirement for linker args specified in `.cargo/config`.
- Added `--verbose` and `--quiet` flags for `build` and `generate-metadata` commands.

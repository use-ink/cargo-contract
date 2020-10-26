# Version v0.7.1 (2020-10-26)

* Update new command template to ink! 3.0-rc2 [#85](https://github.com/paritytech/cargo-contract/pull/85)

# Version v0.7.0 (2020-10-13)

* Fix deprecation warnings [#82](https://github.com/paritytech/cargo-contract/pull/82)
* Use ink 3.0.0-rc1 [#82](https://github.com/paritytech/cargo-contract/pull/82)
* [template] now uses ink_env and ink_storage [#81](https://github.com/paritytech/cargo-contract/pull/81)
* Update new command template to ink! 3.0 syntax [#80](https://github.com/paritytech/cargo-contract/pull/80)
* Extract contract metadata to its own crate [#69](https://github.com/paritytech/cargo-contract/pull/69)
* Fix ManifestPath compiler errors [#73](https://github.com/paritytech/cargo-contract/pull/73)
* Upgrade cargo-xbuild and other dependencies [#71](https://github.com/paritytech/cargo-contract/pull/71)
* Update subxt and async-std dependencies [#66](https://github.com/paritytech/cargo-contract/pull/66)
* Generate extended contract metadata [#62](https://github.com/paritytech/cargo-contract/pull/62)
* Autogenerate abi/metadata package [#58](https://github.com/paritytech/cargo-contract/pull/58)
* Extract workspace to module directory [#59](https://github.com/paritytech/cargo-contract/pull/59)
* Add preferred default release profile settings [#55](https://github.com/paritytech/cargo-contract/pull/55)
* Add option to build with unmodified original manifest [#51](https://github.com/paritytech/cargo-contract/pull/51)
* Update cargo-xbuild [#54](https://github.com/paritytech/cargo-contract/pull/54)

# Version 0.6.1 (2020-05-12)

- Fix LTO regressions in nightly toolchain [#52](https://github.com/paritytech/cargo-contract/pull/52)

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

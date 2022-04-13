# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.2.0] - 2022-04-13

### Added
- `decode` command for event, message and constructor data decoding - [#481](https://github.com/paritytech/cargo-contract/pull/481)

### Fixed
- Fix usage of `check-only` and remove need for `FromStr` impl - [#499](https://github.com/paritytech/cargo-contract/pull/499)

## [1.1.1] - 2022-04-05

### Fixed
- Fix linting support for Apple Silicon (and some other architectures) - [#489](https://github.com/paritytech/cargo-contract/pull/489)
- Allow multiple args values for call and instantiate commands - [#480](https://github.com/paritytech/cargo-contract/pull/480)
- Fix event decoding - [`c721b1`](https://github.com/paritytech/cargo-contract/commit/c721b19519e579de41217aa347625920925d8040)

## [1.1.0] - 2022-03-18

### Added
- `--skip-linting` flag that allows to skip the linting step during build process - [#468](https://github.com/paritytech/cargo-contract/pull/468)

## [1.0.1] - 2022-03-18
- Improved error reporting during installation - [#469](https://github.com/paritytech/cargo-contract/pull/469)

## [1.0.0] - 2022-03-17

### Changed
- Updated `cargo contract new` template dependencies to ink! `3.0` - [#466](https://github.com/paritytech/cargo-contract/pull/466)

## [0.18.0] - 2022-03-14

### Interact with contracts: upload, instantiate and call commands

We added commands to upload, instantiate and call contracts!
This allows interacting with contracts on live chains with a compatible 
[`pallet-contracts`](https://github.com/paritytech/substrate/tree/master/frame/contracts).

For command-line examples on how to use these commands see [#79](https://github.com/paritytech/cargo-contract/pull/79).

### Linting rules for smart contracts

We are introducing a linter for ink! smart contracts in this release!
From now on `cargo-contract` checks if the ink! smart contract that is
`build` or `check`-ed follows certain rules.

As a starting point we've only added one linting rule so far; it asserts correct
initialization of the [`ink_storage::Mapping`](https://paritytech.github.io/ink/ink_storage/struct.Mapping.html)
data structure.

In order for the linting to work with your smart contract, the contract has to be
written in at least ink! 3.0.0-rc9. If it's older the linting will just always succeed.

### Added
- Interact with contracts: upload, instantiate and call commands - [#79](https://github.com/paritytech/cargo-contract/pull/79)
- Add linting to assert correct initialization of [`ink_storage::Mapping`](https://paritytech.github.io/ink/ink_storage/struct.Mapping.html) - [#431](https://github.com/paritytech/cargo-contract/pull/431)

### Changed
- Upgrade `subxt`, SCALE crates, and substrate primitive `sp-*` crates [#451](https://github.com/paritytech/cargo-contract/pull/451).
- Updated `cargo contract new` template dependencies to ink! `3.0.0-rc9` - [#443](https://github.com/paritytech/cargo-contract/pull/443)

## [0.17.0] - 2022-01-19

### Changed
- Updated `cargo contract new` template dependencies to ink! `3.0.0-rc8` - [#402](https://github.com/paritytech/cargo-contract/pull/402)
- Reverted the disabled overflow checks in the `cargo contract new` template - [#376](https://github.com/paritytech/cargo-contract/pull/376)
- Migrated to 2021 edition, enforcing MSRV of `1.56.1` - [#360](https://github.com/paritytech/cargo-contract/pull/360)

### Added
- For contract size optimization added `workspace` section to override parent `workspace` - [#378](https://github.com/paritytech/cargo-contract/pull/378)

## [0.16.0] - 2021-11-25

### Changed
- Updated `cargo contract new` template dependencies to ink! `3.0.0-rc7` - [#374](https://github.com/paritytech/cargo-contract/pull/374)
- Disabled overflow checks in the `cargo contract new` template - [#372](https://github.com/paritytech/cargo-contract/pull/372)
- Use `-Clinker-plugin-lto` if `lto` is enabled (reduces the size of a contract) - [#358](https://github.com/paritytech/cargo-contract/pull/358)
- Deserialize metadata - [#368](https://github.com/paritytech/cargo-contract/pull/368)

### Added
- Added a `--offline` flag to build contracts without network access - [#356](https://github.com/paritytech/cargo-contract/pull/356)

## [0.15.0] - 2021-10-18

### Changed
- Update to `scale-info` 1.0 and support new metadata versioning - [#342](https://github.com/paritytech/cargo-contract/pull/342)
- Update `cargo contract new` template dependencies to ink! `rc6` - [#342](https://github.com/paritytech/cargo-contract/pull/342)

## [0.14.0] - 2021-08-12

### Added
-  Add option for JSON formatted output - [#324](https://github.com/paritytech/cargo-contract/pull/324)

### Changed
- Use new dependency resolver for template contract - [#325](https://github.com/paritytech/cargo-contract/pull/325)
- Do not strip out panic messages in debug builds - [#326](https://github.com/paritytech/cargo-contract/pull/326)

## [0.13.1] - 2021-08-03

### Fixed
- Fixed a Windows issue with contract files in sub-folders - [#313](https://github.com/paritytech/cargo-contract/pull/313)

## [0.13.0] - 2021-07-22

### Added
- Convenient off-chain testing through `cargo contract test` - [#283](https://github.com/paritytech/cargo-contract/pull/283)
- Build contracts in debug mode by default, add `--release` flag - [#298](https://github.com/paritytech/cargo-contract/pull/298)
- Add `--keep-symbols` flag for better Wasm analysis capabilities  - [#302](https://github.com/paritytech/cargo-contract/pull/302)

### Changed
- Change default optimizations pass to focus on code size - [#305](https://github.com/paritytech/cargo-contract/pull/305)

## [0.12.1] - 2021-05-25

### Added
- Suggest `binaryen` installation from GitHub release on outdated version - [#274](https://github.com/paritytech/cargo-contract/pull/274)

### Fixed
- Always use library targets name for contract artifacts - [#277](https://github.com/paritytech/cargo-contract/pull/277)

## [0.12.0] - 2021-04-21

### Fixed
- Fixed `ERROR: The workspace root package should be a workspace member` when building a contract
  under Windows - [#261](https://github.com/paritytech/cargo-contract/pull/261)

### Removed
- Remove support for `--binaryen-as-dependency` - [#251](https://github.com/paritytech/cargo-contract/pull/251)
- Remove support for the deprecated `cargo contract generate-metadata` command - [#265](https://github.com/paritytech/cargo-contract/pull/265)
- Remove pinned `funty` dependency from "new project" template - [#260](https://github.com/paritytech/cargo-contract/pull/260)

## [0.11.1] - 2021-04-06

### Fixed
- Fix `wasm-opt --version` parsing - [#248](https://github.com/paritytech/cargo-contract/pull/248)

## [0.11.0] - 2021-03-31

### Added
- Improve error output for `wasm-opt` interaction - [#244](https://github.com/paritytech/cargo-contract/pull/244)
- Check optimized Wasm output file exists - [#243](https://github.com/paritytech/cargo-contract/pull/243)
- Detect `wasm-opt` version compatibility and improve error messages - [#242](https://github.com/paritytech/cargo-contract/pull/242)
- Detect version mismatches of `parity-scale-codec` in contract and ink! dependency - [#237](https://github.com/paritytech/cargo-contract/pull/237)
- Support specifying `optimization-passes` in the release profile - [#231](https://github.com/paritytech/cargo-contract/pull/231)
- Support specifying `optimization-passes` on the CLI - [#216](https://github.com/paritytech/cargo-contract/pull/216)
- Use `ink::test` attribute in "new project" template - [#190](https://github.com/paritytech/cargo-contract/pull/190)

### Fixed
- Only allow new contract names beginning with an alphabetic character - [#219](https://github.com/paritytech/cargo-contract/pull/219)
- Upgrade `cargo-metadata` and fix usages - [#210](https://github.com/paritytech/cargo-contract/pull/210)

## [0.10.0] - 2021-03-02

### Fixed
- no periods in new contract names - [#192](https://github.com/paritytech/cargo-contract/pull/192)

### Changed
- Update `cargo contract new` template dependencies for `ink!` `rc3` - [#204](https://github.com/paritytech/cargo-contract/pull/204)

## [0.9.1] - 2021-02-24

### Fixed
- Fix linker error when building complex contracts - [#199](https://github.com/paritytech/cargo-contract/pull/199)

## [0.9.0] - 2021-02-22

### Added
- Implement Wasm validation for known issues/markers - [#171](https://github.com/paritytech/cargo-contract/pull/171)

### Changed
- Use either `binaryen-rs` dep or `wasm-opt` binary - [#168](https://github.com/paritytech/cargo-contract/pull/168)
- Update to scale-info 0.5 and codec 2.0 - [#164](https://github.com/paritytech/cargo-contract/pull/164)
- Put build artifacts under `target/ink/` - [#122](https://github.com/paritytech/cargo-contract/pull/122)

### Fixed
- Fix `wasm-opt` regression - [#187](https://github.com/paritytech/cargo-contract/pull/187)
- Generate metadata explicitly for the contract which is build - [#174](https://github.com/paritytech/cargo-contract/pull/174)
- Fix bug with empty Wasm file when using system binaryen for optimization - [#179](https://github.com/paritytech/cargo-contract/pull/179)
- Suppress output on `--quiet` - [#165](https://github.com/paritytech/cargo-contract/pull/165)
- Do not generate build artifacts under `target` for `check` - [#124](https://github.com/paritytech/cargo-contract/pull/124)
- update wasm-path usage name - [#135](https://github.com/paritytech/cargo-contract/pull/135)

## [0.8.0] - 2020-11-27

- Exit with 1 on Err [#109](https://github.com/paritytech/cargo-contract/pull/109)
- Use package name instead of lib name for metadata dependency [#107](https://github.com/paritytech/cargo-contract/pull/107)
- Do not prettify JSON for bundle [#105](https://github.com/paritytech/cargo-contract/pull/105)
- Make `source.hash` non-optional, remove metadata-only [#104](https://github.com/paritytech/cargo-contract/pull/104)
- Implement new commands `build` and `check` + introduce bundles (.contract files) [#97](https://github.com/paritytech/cargo-contract/pull/97)
- Replace xbuild with cargo build-std [#99](https://github.com/paritytech/cargo-contract/pull/99)
- Use binaryen-rs as dep instead of requiring manual wasm-opt installation [#95](https://github.com/paritytech/cargo-contract/pull/95)
- Specify optional --manifest-path for build and generate-metadata [#93](https://github.com/paritytech/cargo-contract/pull/93)

## [0.7.1] - 2020-10-26

- Update new command template to ink! 3.0-rc2 [#85](https://github.com/paritytech/cargo-contract/pull/85)

## [0.7.0] - 2020-10-13

- Fix deprecation warnings [#82](https://github.com/paritytech/cargo-contract/pull/82)
- Use ink 3.0.0-rc1 [#82](https://github.com/paritytech/cargo-contract/pull/82)
- [template] now uses ink_env and ink_storage [#81](https://github.com/paritytech/cargo-contract/pull/81)
- Update new command template to ink! 3.0 syntax [#80](https://github.com/paritytech/cargo-contract/pull/80)
- Extract contract metadata to its own crate [#69](https://github.com/paritytech/cargo-contract/pull/69)
- Fix ManifestPath compiler errors [#73](https://github.com/paritytech/cargo-contract/pull/73)
- Upgrade cargo-xbuild and other dependencies [#71](https://github.com/paritytech/cargo-contract/pull/71)
- Update subxt and async-std dependencies [#66](https://github.com/paritytech/cargo-contract/pull/66)
- Generate extended contract metadata [#62](https://github.com/paritytech/cargo-contract/pull/62)
- Autogenerate abi/metadata package [#58](https://github.com/paritytech/cargo-contract/pull/58)
- Extract workspace to module directory [#59](https://github.com/paritytech/cargo-contract/pull/59)
- Add preferred default release profile settings [#55](https://github.com/paritytech/cargo-contract/pull/55)
- Add option to build with unmodified original manifest [#51](https://github.com/paritytech/cargo-contract/pull/51)
- Update cargo-xbuild [#54](https://github.com/paritytech/cargo-contract/pull/54)

## [0.6.1] - 2020-05-12

- Fix LTO regressions in nightly toolchain [#52](https://github.com/paritytech/cargo-contract/pull/52)

## [0.6.0] - 2020-03-25

- First release to crates.io
- Use `subxt` release from [crates.io](https://crates.io/crates/substrate-subxt)

## [0.5.0] - 2020-03-18

- Upgrades dependencies [#45](https://github.com/paritytech/cargo-contract/pull/45)
- Update template to ink! 2.0 dependencies [#47](https://github.com/paritytech/cargo-contract/pull/47)

## [0.4.1] - 2020-02-26

- Fix: fail the whole build process if the contract build fails.

## [0.4.0] - 2020-02-26

- Minimize contract wasm binary size:
  - Run `wasm-opt` on the contract Wasm binary.
  - Uses [`cargo-xbuild`](https://github.com/rust-osdev/cargo-xbuild) to build custom sysroot crates without panic string
bloat.
  - Automatically removes the `rlib` crate type from `Cargo.toml`, removing redundant metadata.
- Removes requirement for linker args specified in `.cargo/config`.
- Added `--verbose` and `--quiet` flags for `build` and `generate-metadata` commands.

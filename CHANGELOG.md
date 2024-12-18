# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

[Unreleased]

## [5.0.2]

### Changed
- Target `pallet-revive` instead of `pallet-contracts` - [#1851](https://github.com/use-ink/cargo-contract/pull/1851)

### Added
- Add `suri-path` and `password-path` options for `cargo contract` commands including `upload`, `instantiate`, `call`, and `remove`

## [5.0.1]

### Changed
- Bumped the ink! dependencies to ink! 5.1.0 - [#1837](https://github.com/use-ink/cargo-contract/pull/1837)
- Synchronized the `sp-*` dependencies with the ones used in ink! 5.1.0 - [#1837](https://github.com/use-ink/cargo-contract/pull/1837)

## [5.0.0]

This release concludes the migration of ink! from Parity to the outside world. It doesn't come with any new features, we just:

* …changed the Parity URLs to ones for our new GitHub organization
[@use-ink](https://github.com/use-ink/).
* …upgraded many dependencies to newer versions, which results in two particular
  breaking changes regarding compatibility:
  * We had to remove support for Substrate metadata that is below
      `V14` in [#1722](https://github.com/use-ink/cargo-contract/pull/1722). Metadata formats below `V14` are quite old and we hope this doesn't affect anyone.
  * `cargo-contract` v5 works only with Rust >= 1.81.

For the linter in `cargo-contract` the Rust toolchain version changed.
To upgrade:

```
export TOOLCHAIN_VERSION=nightly-2024-09-05
rustup install $TOOLCHAIN_VERSION
rustup component add rust-src --toolchain $TOOLCHAIN_VERSION
rustup run $TOOLCHAIN_VERSION cargo install cargo-dylint dylint-link
```

### Changed
- Updated the toolchain version used by `ink_linting` - [#1616](https://github.com/use-ink/cargo-contract/pull/1616)
- Update repository URLs & references from `paritytech` GitHub organization to new `use-ink` one ‒ [#1663](https://github.com/use-ink/cargo-contract/pull/1663)
- Bump the version of `subxt` and `subxt-signer` - [#1722](https://github.com/use-ink/cargo-contract/pull/1722)

### Removed
- Remove support for `V11` metadata [#1722](https://github.com/use-ink/cargo-contract/pull/1722)

## [4.1.1]

### Fixed
- Remove mention of non-existent argument, improve clarity of warning message - [#1590](https://github.com/use-ink/cargo-contract/pull/1590)

## [4.1.0]

### Fixed
- Fix the `instantiate` command for Substrate `0.9.42` based chains - [#1564](https://github.com/use-ink/cargo-contract/pull/1564)

### Added
- Add `cargo contract storage --version` command - [#1564](https://github.com/use-ink/cargo-contract/pull/1564)
- Add `cargo contract verify --wasm` argument - [#1551](https://github.com/use-ink/cargo-contract/pull/1551)
- Add `cargo contract instantiate --chain` with production chain endpoints - [#1290](https://github.com/use-ink/cargo-contract/pull/1290)
- Warn when uploading unverifiable contract builds to production - [#1290](https://github.com/use-ink/cargo-contract/pull/1290)

## [4.0.2]

### Fixed
- Fix installation instructions for `ink_linting` - [#1546](https://github.com/use-ink/cargo-contract/pull/1546)

## [4.0.1]

### Fixed
- Fix e2e tests in the contract template - [#1537](https://github.com/use-ink/cargo-contract/pull/1537)

## [4.0.0]

This `cargo-contract` release is compatible with Rust versions `>=1.70`and ink! versions `>=5.0.0`

ℹ️ _We've created a migration guide from ink! 4 to ink! 5. It also contains an
overview over newly added features in this release of `cargo-contract` and explains
e.g. the newly added contract verification in more detail._

👉 _You can view it [here](https://use.ink/faq/migrating-from-ink-4-to-5)._

**Notable changes:**
- Verifiable builds inside a docker container - [#1148](https://github.com/use-ink/cargo-contract/pull/1148)
- Extrinsics extracted to separate crate - [#1181](https://github.com/use-ink/cargo-contract/pull/1181)
- Fix building contracts with Rust >= 1.70: enable `sign_ext` Wasm opcode - [#1189](https://github.com/use-ink/cargo-contract/pull/1189)
- Support for multiple versions of `pallet-contracts` - [#1399](https://github.com/use-ink/cargo-contract/pull/1399)

### Added
- Export `ink_metadata` types in `transcode` crate - [#1522](https://github.com/use-ink/cargo-contract/pull/1522)
- Improved error message for Strings as CLI arguments - [#1492](https://github.com/use-ink/cargo-contract/pull/1492)
- Add a user-friendly view of contract storage data in the form of a table - [#1414](https://github.com/use-ink/cargo-contract/pull/1414)
- Add `rpc` command - [#1458](https://github.com/use-ink/cargo-contract/pull/1458)
- Add schema generation and verification - [#1404](https://github.com/use-ink/cargo-contract/pull/1404)
- Compare `Environment` types against the node - [#1377](https://github.com/use-ink/cargo-contract/pull/1377)
- Detect `INK_STATIC_BUFFER_SIZE` env var - [#1310](https://github.com/use-ink/cargo-contract/pull/1310)
- Add `verify` command - [#1306](https://github.com/use-ink/cargo-contract/pull/1306)
- Add `--binary` flag for `info` command - [#1311](https://github.com/use-ink/cargo-contract/pull/1311/)
- Add `--all` flag for `info` command - [#1319](https://github.com/use-ink/cargo-contract/pull/1319)
- Add contract language detection feature for `info` command - [#1329](https://github.com/use-ink/cargo-contract/pull/1329)
- Add warning message when using incompatible contract's ink! version - [#1334](https://github.com/use-ink/cargo-contract/pull/1334)
- Add workspace support -[#1358](https://github.com/use-ink/cargo-contract/pull/1358)
- Add `Storage Total Deposit` to `info` command output - [#1347](https://github.com/use-ink/cargo-contract/pull/1347)
- Add dynamic types support - [#1399](https://github.com/use-ink/cargo-contract/pull/1399)
- Basic storage inspection command - [#1395](https://github.com/use-ink/cargo-contract/pull/1395)
- Standardised verifiable builds - [#1148](https://github.com/use-ink/cargo-contract/pull/1148)
- Enable Wasm sign_ext [#1189](https://github.com/use-ink/cargo-contract/pull/1189)
- Expose extrinsics operations as a library - [#1181](https://github.com/use-ink/cargo-contract/pull/1181)
- Suggest valid message or constructor name, when misspelled - [#1162](https://github.com/use-ink/cargo-contract/pull/1162)
- Add flag -y as a shortcut for --skip-confirm - [#1127](https://github.com/use-ink/cargo-contract/pull/1127)
- Add command line argument --max-memory-pages - [#1128](https://github.com/use-ink/cargo-contract/pull/1128)
- Show Gas consumption by default for dry-runs - [#1121](https://github.com/use-ink/cargo-contract/pull/1121)

### Changed
- Print type comparison warning only on `--verbose` - [#1483](https://github.com/use-ink/cargo-contract/pull/1483)
- Mandatory dylint-based lints - [#1412](https://github.com/use-ink/cargo-contract/pull/1412)
- Add a new tabular layout for the contract storage data - [#1485](https://github.com/use-ink/cargo-contract/pull/1485)
- Run wasm-opt first, remove sign_ext feature - [#1416](https://github.com/use-ink/cargo-contract/pull/1416)
- Bump `subxt` to `0.32.0` - [#1352](https://github.com/use-ink/cargo-contract/pull/1352)
- Remove check for compatible `scale` and `scale-info` versions - [#1370](https://github.com/use-ink/cargo-contract/pull/1370)
- Dry-run result output improvements - [1123](https://github.com/use-ink/cargo-contract/pull/1123)
- Display build progress with --output-json, print to stderr - [1211](https://github.com/use-ink/cargo-contract/pull/1211)
- Upgrade wasm-opt to 0.113 - [#1188](https://github.com/use-ink/cargo-contract/pull/1188)
- Update substrate dependencies - [#1149](https://github.com/use-ink/cargo-contract/pull/1149)
- Make output format of cargo contract info consistent with other subcommands - [#1120](https://github.com/use-ink/cargo-contract/pull/1120)
- set minimum supported `rust-version` to `1.70` - [#1241](https://github.com/use-ink/cargo-contract/pull/1241)

### Fixed
- Fix parsing of docker STDOUT - [#1526](https://github.com/use-ink/cargo-contract/pull/1526)
- Remove docker container on build failure - [#1531](https://github.com/use-ink/cargo-contract/pull/1531)
- Fix build `--verifiable` command [#1511](https://github.com/use-ink/cargo-contract/pull/1511)
- Do not allow to execute calls on immutable contract messages - [#1397](https://github.com/use-ink/cargo-contract/pull/1397)
- Improve JSON Output for Upload and Remove Commands - [#1389](https://github.com/use-ink/cargo-contract/pull/1389)
- Fix for a Url to String conversion in `info` command - [#1330](https://github.com/use-ink/cargo-contract/pull/1330)
- Configure tty output correctly - [#1209](https://github.com/use-ink/cargo-contract/pull/1209)
- Set `lto = "thin"` for metadata build to fix `linkme` on macOS - [#1200](https://github.com/use-ink/cargo-contract/pull/1200)
- fix(build): An error when running with `--lint` - [#1174](https://github.com/use-ink/cargo-contract/pull/1174)
- Dry-run result output improvements - [#1123](https://github.com/use-ink/cargo-contract/pull/1123)
- feat: use `CARGO_ENCODED_RUSTFLAGS` instead of `RUSTFLAGS` - [#1124](https://github.com/use-ink/cargo-contract/pull/1124)

## [4.0.0-rc.4]

### Added
- Export `ink_metadata` types in `transcode` crate - [#1522](https://github.com/use-ink/cargo-contract/pull/1522)

### Fixed
- Fix parsing of docker STDOUT - [#1526](https://github.com/use-ink/cargo-contract/pull/1526)
- Remove docker container on build failure - [#1531](https://github.com/use-ink/cargo-contract/pull/1531)

## [4.0.0-rc.3]

### Fixed
- Fix build `--verifiable` command [#1511](https://github.com/use-ink/cargo-contract/pull/1511)

## [4.0.0-rc.2]

### Added
- Improved error message for Strings as CLI arguments - [#1492](https://github.com/use-ink/cargo-contract/pull/1492)
- Add a user-friendly view of contract storage data in the form of a table - [#1414](https://github.com/use-ink/cargo-contract/pull/1414)
- Add `rpc` command - [#1458](https://github.com/use-ink/cargo-contract/pull/1458)

### Changed
- Print type comparison warning only on `--verbose` - [#1483](https://github.com/use-ink/cargo-contract/pull/1483)
- Mandatory dylint-based lints - [#1412](https://github.com/use-ink/cargo-contract/pull/1412)
- Add a new tabular layout for the contract storage data - [#1485](https://github.com/use-ink/cargo-contract/pull/1485)

## [4.0.0-rc.1]

### Changed
- Run wasm-opt first, remove sign_ext feature - [#1416](https://github.com/use-ink/cargo-contract/pull/1416)

## [4.0.0-rc]

### Added
- Add schema generation and verification - [#1404](https://github.com/use-ink/cargo-contract/pull/1404)
- Compare `Environment` types against the node - [#1377](https://github.com/use-ink/cargo-contract/pull/1377)
- Detect `INK_STATIC_BUFFER_SIZE` env var - [#1310](https://github.com/use-ink/cargo-contract/pull/1310)
- Add `verify` command - [#1306](https://github.com/use-ink/cargo-contract/pull/1306)
- Add `--binary` flag for `info` command - [#1311](https://github.com/use-ink/cargo-contract/pull/1311/)
- Add `--all` flag for `info` command - [#1319](https://github.com/use-ink/cargo-contract/pull/1319)
- Add contract language detection feature for `info` command - [#1329](https://github.com/use-ink/cargo-contract/pull/1329)
- Add warning message when using incompatible contract's ink! version - [#1334](https://github.com/use-ink/cargo-contract/pull/1334)
- Add workspace support -[#1358](https://github.com/use-ink/cargo-contract/pull/1358)
- Add `Storage Total Deposit` to `info` command output - [#1347](https://github.com/use-ink/cargo-contract/pull/1347)
- Add dynamic types support - [#1399](https://github.com/use-ink/cargo-contract/pull/1399)
- Basic storage inspection command - [#1395](https://github.com/use-ink/cargo-contract/pull/1395)

### Changed
- Bump `subxt` to `0.32.0` - [#1352](https://github.com/use-ink/cargo-contract/pull/1352)
- Remove check for compatible `scale` and `scale-info` versions - [#1370](https://github.com/use-ink/cargo-contract/pull/1370)
- Bump `ink` to `5.0.0-rc` - [#1415](https://github.com/use-ink/cargo-contract/pull/1415)

### Fixed
- Do not allow to execute calls on immutable contract messages - [#1397](https://github.com/use-ink/cargo-contract/pull/1397)
- Improve JSON Output for Upload and Remove Commands - [#1389](https://github.com/use-ink/cargo-contract/pull/1389)
- Fix for a Url to String conversion in `info` command - [#1330](https://github.com/use-ink/cargo-contract/pull/1330)

## [4.0.0-alpha]

Replaces the yanked `3.1.0` due to issues with supporting *both* Rust versions < `1.70`
and >= `1.70`.

If you intend to use `cargo-contract` with Rust >= `1.70`, and deploy to a node with a
version of `pallet-contracts` at least `polkadot-1.0.0`, then this is the release to use.

If you still want to compile with `1.69` and target an older version of `pallet-contracts`
then use the previous `3.0.1` release.

**Notable changes:**
- Verifiable builds inside a docker container - [#1148](https://github.com/use-ink/cargo-contract/pull/1148)
- Extrinsics extracted to separate crate - [#1181](https://github.com/use-ink/cargo-contract/pull/1181)
- Fix building contracts with Rust >= 1.70: enable `sign_ext` Wasm opcode - [#1189](https://github.com/use-ink/cargo-contract/pull/1189)

### Added
- Standardised verifiable builds - [#1148](https://github.com/use-ink/cargo-contract/pull/1148)
- Enable Wasm sign_ext [#1189](https://github.com/use-ink/cargo-contract/pull/1189)
- Expose extrinsics operations as a library - [#1181](https://github.com/use-ink/cargo-contract/pull/1181)
- Suggest valid message or constructor name, when misspelled - [#1162](https://github.com/use-ink/cargo-contract/pull/1162)
- Add flag -y as a shortcut for --skip-confirm - [#1127](https://github.com/use-ink/cargo-contract/pull/1127)
- Add command line argument --max-memory-pages - [#1128](https://github.com/use-ink/cargo-contract/pull/1128)
- Show Gas consumption by default for dry-runs - [#1121](https://github.com/use-ink/cargo-contract/pull/1121)

### Changed
- Dry-run result output improvements - [1123](https://github.com/use-ink/cargo-contract/pull/1123)
- Display build progress with --output-json, print to stderr - [1211](https://github.com/use-ink/cargo-contract/pull/1211)
- Update `subxt` to `0.30.1` with new `subxt-signer` crate - [#1236](https://github.com/use-ink/cargo-contract/pull/1236)
- Upgrade wasm-opt to 0.113 - [#1188](https://github.com/use-ink/cargo-contract/pull/1188)
- Update substrate dependencies - [#1149](https://github.com/use-ink/cargo-contract/pull/1149)
- Make output format of cargo contract info consistent with other subcommands - [#1120](https://github.com/use-ink/cargo-contract/pull/1120)
- set minimum supported `rust-version` to `1.70` - [#1241](https://github.com/use-ink/cargo-contract/pull/1241)
- `[extrinsics]` update metadata to `substrate-contracts-node 0.29` - [#1242](https://github.com/use-ink/cargo-contract/pull/1242)

### Fixed
- Configure tty output correctly - [#1209](https://github.com/use-ink/cargo-contract/pull/1209)
- Set `lto = "thin"` for metadata build to fix `linkme` on macOS - [#1200](https://github.com/use-ink/cargo-contract/pull/1200)
- fix(build): An error when running with `--lint` - [#1174](https://github.com/use-ink/cargo-contract/pull/1174)
- Dry-run result output improvements - [#1123](https://github.com/use-ink/cargo-contract/pull/1123)
- feat: use `CARGO_ENCODED_RUSTFLAGS` instead of `RUSTFLAGS` - [#1124](https://github.com/use-ink/cargo-contract/pull/1124)

## [3.1.0] **YANKED**

## [3.0.1]

### Fixed
- `[contract-build]` flush the remaining buffered bytes - [1118](https://github.com/use-ink/cargo-contract/pull/1118)

## [3.0.0]

### Added
- Experimental support for RISC-V contracts - [#1076](https://github.com/use-ink/cargo-contract/pull/1076)

### Changed
- Contracts are build as `bin` crate now (we used `cdylib` before) - [#1076](https://github.com/use-ink/cargo-contract/pull/1076)
  - BREAKING CHANGE: Make sure that your contract is `no_main` by having this on top of your contract:
    - `#![cfg_attr(not(feature = "std"), no_std, no_main)]`
    - This will be detected and suggested for `error[E0601]` - [#1113](https://github.com/use-ink/cargo-contract/pull/1113)
- Update contracts node metadata (#1105)
  - Compatible with `substrate-contracts-node 0.25.0-a2b09462c7c`

### Fixed
- Fix original Wasm artifact path [#1116](https://github.com/use-ink/cargo-contract/pull/1116)

## [2.2.1]

### Fixed
- Revert "Bump tracing from 0.1.37 to 0.1.38" - [#1096](https://github.com/use-ink/cargo-contract/pull/1096)

## [2.2.0]

### Added
- Add `info` command - [#993](https://github.com/use-ink/cargo-contract/pull/993)
- Add `--output-json` flag for `info` command - [#1007](https://github.com/use-ink/cargo-contract/pull/1007)

### Changed
- Minimum requirements of `ink!` dependencies all updated to `4.2.0` - [#1084](https://github.com/use-ink/cargo-contract/pull/1084)
- Upgrade `subxt` to `0.28` [#1039](https://github.com/use-ink/cargo-contract/pull/1039)
- Upgrade `scale-info` to `2.5` [#1057](https://github.com/use-ink/cargo-contract/pull/1057)

### Fixed
- Rewrites build file path in manifest [#1077](https://github.com/use-ink/cargo-contract/pull/1077)
- Only copy and rewrite target contract manifest [#1079](https://github.com/use-ink/cargo-contract/pull/1079)

## [2.1.0]

### Changed
- Dry-run `instantiate`, `call` and `upload` commands by default - [#999](https://github.com/use-ink/cargo-contract/pull/999)

### Added
- Add `cargo contract encode` command - [#998](https://github.com/use-ink/cargo-contract/pull/998)

### Fixed
- Limit input length for `decode` command - [#982](https://github.com/use-ink/cargo-contract/pull/982)
- Pass contract features to metadata gen package - [#1005](https://github.com/use-ink/cargo-contract/pull/1005)
- Custom AccountId32 impl, remove substrate deps - [#1010](https://github.com/use-ink/cargo-contract/pull/1010)
  - Fixes issue with with incompatible `wasmtime` versions when dependant project has old substrate dependencies.

### [2.0.2]

### Fixed
- Explicitly enable `std` feature for metadata generation [#977](https://github.com/use-ink/cargo-contract/pull/977)
- Return artifact paths when contracts unchanged [#992](https://github.com/use-ink/cargo-contract/pull/992)
- Minimum requirements of `ink!` dependencies all updated to `4.0.1`

## [2.0.1]

### Fixed
- Return correct contract id for `instantiate` command with subcontracts ‒ [#777](https://github.com/use-ink/cargo-contract/pull/777)
- Bump template to ink! 4.0 ‒ [#971](https://github.com/use-ink/cargo-contract/pull/971)

## [2.0.0]

Major release compatible with `ink! 4.0.0`. All the changes in aggregate since `1.5`:

### Added
- Add support for ink!'s `version` metadata field - [#641](https://github.com/use-ink/cargo-contract/pull/641)
- Add Rust specific build info to metadata - [#680](https://github.com/use-ink/cargo-contract/pull/680)
- Log code hash if contract is already uploaded - [#805](https://github.com/use-ink/cargo-contract/pull/805)
- Add remove command - [#837](https://github.com/use-ink/cargo-contract/pull/837)

### Changed
- Build contracts and dylint driver with stable - [#698](https://github.com/use-ink/cargo-contract/pull/698)
- Compile dylints when compiling the contract - [#703](https://github.com/use-ink/cargo-contract/pull/703)
- Move transcode example to doc test, add helper method - [#705](https://github.com/use-ink/cargo-contract/pull/705)
  - Note that alongside this PR we released [`contract-transcode@0.2.0`](https://crates.io/crates/contract-transcode/0.2.0)
- Replace custom RPCs by `state_call` - [#701](https://github.com/use-ink/cargo-contract/pull/701)
- Removed requirement to install binaryen. The `wasm-opt` tool is now compiled into `cargo-contract` - [#766](https://github.com/use-ink/cargo-contract/pull/766).
- Make linting opt in with `--lint` - [#799](https://github.com/use-ink/cargo-contract/pull/799)
- Update to weights v2 - [#809](https://github.com/use-ink/cargo-contract/pull/809)
- Update validation for renamed FFI functions - [#816](https://github.com/use-ink/cargo-contract/pull/816)
- Denominated units for balances in events - [#750](https://github.com/use-ink/cargo-contract/pull/750)
- Upgrade wasm-opt to 0.110.2 - [#802](https://github.com/use-ink/cargo-contract/pull/802)
- Pass `--features` through to `cargo` - [#853](https://github.com/use-ink/cargo-contract/pull/853/files)
- Bump minimum requirement of `scale-info` in template to `2.3` - [#847](https://github.com/use-ink/cargo-contract/pull/847/files)
- Remove `unstable` module check, add `--skip-wasm-validation` - [#846](https://github.com/use-ink/cargo-contract/pull/846/files)
- Extract lib for invoking contract build - [#787](https://github.com/use-ink/cargo-contract/pull/787/files)
- Upgrade wasm-opt to 0.111.0 [#888](https://github.com/use-ink/cargo-contract/pull/888)
- Enable `wasm-opt` MVP features only [#891](https://github.com/use-ink/cargo-contract/pull/891)
- Require env_type transcoders to be Send + Sync [#879](https://github.com/use-ink/cargo-contract/pull/879)
- Extrinsics: allow specifying contract artifact directly [#893](https://github.com/use-ink/cargo-contract/pull/893)
- Upgrade `subxt` to `0.26` [#924](https://github.com/use-ink/cargo-contract/pull/924)
- Display detailed cause of an error [#931](https://github.com/use-ink/cargo-contract/pull/931)
- Use package name instead of lib name, default to "rlib" [#929](https://github.com/use-ink/cargo-contract/pull/929)
- Rename `metadata.json` to `{contract_name}.json` - [#952](https://github.com/use-ink/cargo-contract/pull/952)
- Do not postprocess or generate metadata if contract unchanged [#964](https://github.com/use-ink/cargo-contract/pull/964)
- Update `subxt` and substrate dependencies [#968](https://github.com/use-ink/cargo-contract/pull/968)

### Fixed
- Fix `tracing_subscriber` filtering - [#702](https://github.com/use-ink/cargo-contract/pull/702)
- Sync version of transcode crate to fix metadata parsing - [#723](https://github.com/use-ink/cargo-contract/pull/723)
- Fix numbering of steps during `build` - [#715](https://github.com/use-ink/cargo-contract/pull/715)
- Skip linting if running building in --offline mode -  [#737](https://github.com/use-ink/cargo-contract/pull/737)
- Fix storage deposit limit encoding - [#751](https://github.com/use-ink/cargo-contract/pull/751)
- Rewrite relative path for `dev-dependency` - [#760](https://github.com/use-ink/cargo-contract/pull/760)
- Log failure instead of failing when decoding an event - [#769](https://github.com/use-ink/cargo-contract/pull/769)
- Fixed having non-JSON output after calling `instantiate` with `--output-json` - [#839](https://github.com/use-ink/cargo-contract/pull/839/files)
- add `-C target-cpu=mvp` rust flag to build command - [#838](https://github.com/use-ink/cargo-contract/pull/838/files)
- Miscellaneous extrinsics display improvements [#916](https://github.com/use-ink/cargo-contract/pull/916)
- Fix decoding of `LangError` [#919](https://github.com/use-ink/cargo-contract/pull/919)
- Respect the lockfile [#948](https://github.com/use-ink/cargo-contract/pull/948)
- Error if mismatching # of args for instantiate/call [#966](https://github.com/use-ink/cargo-contract/pull/966)
- Pretty-print call dry-run return data [#967](https://github.com/use-ink/cargo-contract/pull/967)

### Removed
- Remove the `test` command [#958](https://github.com/use-ink/cargo-contract/pull/958)
- Remove rust toolchain channel check - [#848](https://github.com/use-ink/cargo-contract/pull/848/files)

## [2.0.0-rc.1] - 2023-02-01
Second release candidate compatible with `ink! 4.0.0-rc`.

### Changed
- Upgrade `subxt` to `0.26` [#924](https://github.com/use-ink/cargo-contract/pull/924)
- Display detailed cause of an error [#931](https://github.com/use-ink/cargo-contract/pull/931)
- Use package name instead of lib name, default to "rlib" [#929](https://github.com/use-ink/cargo-contract/pull/929)

### Fixed
- Miscellaneous extrinsics display improvements [#916](https://github.com/use-ink/cargo-contract/pull/916)
- Fix decoding of `LangError` [#919](https://github.com/use-ink/cargo-contract/pull/919)

## [2.0.0-rc] - 2023-01-12

First release candidate for compatibility with `ink! 4.0-rc`.

### Changed
-  Extrinsics: allow specifying contract artifact directly [#893](https://github.com/use-ink/cargo-contract/pull/893)

### Added
- Add `cargo contract remove` command [#837](https://github.com/use-ink/cargo-contract/pull/837)

## [2.0.0-beta.2] - 2023-01-09

### Changed
- Upgrade wasm-opt to 0.111.0 [#888](https://github.com/use-ink/cargo-contract/pull/888)
- Enable `wasm-opt` MVP features only [#891](https://github.com/use-ink/cargo-contract/pull/891)
- Require env_type transcoders to be Send + Sync [#879](https://github.com/use-ink/cargo-contract/pull/879)

### Fixed
- Add determinism arg to upload TX [#870](https://github.com/use-ink/cargo-contract/pull/870)

## [2.0.0-beta.1] - 2022-12-07

### Changed
- Pass `--features` through to `cargo` - [#853](https://github.com/use-ink/cargo-contract/pull/853/files)
- Remove rust toolchain channel check - [#848](https://github.com/use-ink/cargo-contract/pull/848/files)
- Bump minimum requirement of `scale-info` in template to `2.3` - [#847](https://github.com/use-ink/cargo-contract/pull/847/files)
- Remove `unstable` module check, add `--skip-wasm-validation` - [#846](https://github.com/use-ink/cargo-contract/pull/846/files)
- Extract lib for invoking contract build - [#787](https://github.com/use-ink/cargo-contract/pull/787/files)

### Fixed
- Fixed having non-JSON output after calling `instantiate` with `--output-json` - [#839](https://github.com/use-ink/cargo-contract/pull/839/files)
- add `-C target-cpu=mvp` rust flag to build command - [#838](https://github.com/use-ink/cargo-contract/pull/838/files)

## [2.0.0-beta] - 2022-11-22

This release supports the ink! [`v4.0.0-beta`](https://github.com/use-ink/ink/releases/tag/v4.0.0-beta) release.

### Changed
- Update to weights v2 - [#809](https://github.com/use-ink/cargo-contract/pull/809)
- Update validation for renamed FFI functions - [#816](https://github.com/use-ink/cargo-contract/pull/816)
- Denominated units for balances in events - [#750](https://github.com/use-ink/cargo-contract/pull/750)
- Upgrade wasm-opt to 0.110.2 - [#802](https://github.com/use-ink/cargo-contract/pull/802)

### Added
- Log code hash if contract is already uploaded - [#805](https://github.com/use-ink/cargo-contract/pull/805)

## [2.0.0-alpha.5] - 2022-10-27

### Added
- Add Rust specific build info to metadata - [#680](https://github.com/use-ink/cargo-contract/pull/680)

### Changed
- Removed requirement to install binaryen. The `wasm-opt` tool is now compiled into `cargo-contract` - [#766](https://github.com/use-ink/cargo-contract/pull/766).
- Make linting opt in with `--lint` - [#799](https://github.com/use-ink/cargo-contract/pull/799)

### Fixed
-  Log failure instead of failing when decoding an event - [#769](https://github.com/use-ink/cargo-contract/pull/769)

## [2.0.0-alpha.4] - 2022-10-03

### Fixed
- Fix storage deposit limit encoding - [#751](https://github.com/use-ink/cargo-contract/pull/751)
- Rewrite relative path for `dev-dependency` - [#760](https://github.com/use-ink/cargo-contract/pull/760)

## [2.0.0-alpha.3] - 2022-09-21

This release supports compatibility with the [`v4.0.0-alpha.3`](https://github.com/use-ink/ink/releases/tag/v4.0.0-alpha.3)
release of `ink!`. It is *not* backwards compatible with older versions of `ink!`.

### Added
- `--output-json` support for `call`, `instantiate` and `upload` commands - [#722](https://github.com/use-ink/cargo-contract/pull/722)
- Denominated units for Balances - [#750](https://github.com/use-ink/cargo-contract/pull/750)
- Use new ink entrance crate - [#728](https://github.com/use-ink/cargo-contract/pull/728)

### Fixed
- Skip linting if running building in --offline mode -  [#737](https://github.com/use-ink/cargo-contract/pull/737)

## [2.0.0-alpha.2] - 2022-09-02

### Fixed
- Sync version of transcode crate to fix metadata parsing - [#723](https://github.com/use-ink/cargo-contract/pull/723)
- Fix numbering of steps during `build` - [#715](https://github.com/use-ink/cargo-contract/pull/715)

## [2.0.0-alpha.1] - 2022-08-24

This release brings two exciting updates! First, contracts can now be built using the
`stable` Rust toolchain! Don't ask us how we managed to do this 👻.

Secondly, it allows you to build ink! `v4.0.0-alpha.1`, which introduced a small - but
breaking - change to the ink! ABI as part of [use-ink/ink#1313](https://github.com/use-ink/ink/pull/1313).

### Added
- Add support for ink!'s `version` metadata field - [#641](https://github.com/use-ink/cargo-contract/pull/641)

### Changed
- Build contracts and dylint driver with stable - [#698](https://github.com/use-ink/cargo-contract/pull/698)
- Compile dylints when compiling the contract - [#703](https://github.com/use-ink/cargo-contract/pull/703)
- Move transcode example to doc test, add helper method - [#705](https://github.com/use-ink/cargo-contract/pull/705)
    - Note that alongside this PR we released [`contract-transcode@0.2.0`](https://crates.io/crates/contract-transcode/0.2.0)
- Replace custom RPCs by `state_call` - [#701](https://github.com/use-ink/cargo-contract/pull/701)

### Fixed
- Fix `tracing_subscriber` filtering - [#702](https://github.com/use-ink/cargo-contract/pull/702)

## [1.5.0] - 2022-08-15

### Added
- Dry run gas limit estimation - [#484](https://github.com/use-ink/cargo-contract/pull/484)

### Changed
- Bump `ink_*` crates to `v3.3.1` - [#686](https://github.com/use-ink/cargo-contract/pull/686)
- Refactor out transcode as a separate library - [#597](https://github.com/use-ink/cargo-contract/pull/597)
- Sync `metadata` version with `cargo-contract` - [#611](https://github.com/use-ink/cargo-contract/pull/611)
- Adapt to new subxt API - [#678](https://github.com/use-ink/cargo-contract/pull/678)
- Replace log/env_logger with tracing/tracing_subscriber - [#689](https://github.com/use-ink/cargo-contract/pull/689)
- Contract upload: emitting a warning instead of an error when the contract already
  existed is more user friendly - [#644](https://github.com/use-ink/cargo-contract/pull/644)

### Fixed
- Fix windows dylint build [#690](https://github.com/use-ink/cargo-contract/pull/690)
- Fix `instantiate_with_code` with already uploaded code [#594](https://github.com/use-ink/cargo-contract/pull/594)


## [1.4.0] - 2022-05-18

### Changed
- Updated `cargo contract new` template dependencies to ink! `version = "3"` - [#569](https://github.com/use-ink/cargo-contract/pull/569)
- Improved documentation on how to invoke `cargo contract decode` - [#572](https://github.com/use-ink/cargo-contract/pull/572)

### Fixed
- Make constructor selector look for exact function name - [#562](https://github.com/use-ink/cargo-contract/pull/562) (thanks [@forgetso](https://github.com/forgetso)!)
- Fix dirty directory issue when crate installation had been interrupted - [#571](https://github.com/use-ink/cargo-contract/pull/571)

## [1.3.0] - 2022-05-09

### Added
- Allow hex literals for unsigned integers - [#547](https://github.com/use-ink/cargo-contract/pull/547)

### Fixed
- Display `H256` instances in events as hex encoded string - [#550](https://github.com/use-ink/cargo-contract/pull/550)
- Fix extrinsic params for contract chains - [#523](https://github.com/use-ink/cargo-contract/pull/523)
- Fix `Vec<AccountId>` args - [#519](https://github.com/use-ink/cargo-contract/pull/519)
- Fix `--dry-run` error deserialization and report error details - [#534](https://github.com/use-ink/cargo-contract/pull/534)

## [1.2.0] - 2022-04-13

### Added
- `decode` command for event, message and constructor data decoding - [#481](https://github.com/use-ink/cargo-contract/pull/481)

### Fixed
- Fix usage of `check-only` and remove need for `FromStr` impl - [#499](https://github.com/use-ink/cargo-contract/pull/499)

## [1.1.1] - 2022-04-05

### Fixed
- Fix linting support for Apple Silicon (and some other architectures) - [#489](https://github.com/use-ink/cargo-contract/pull/489)
- Allow multiple args values for call and instantiate commands - [#480](https://github.com/use-ink/cargo-contract/pull/480)
- Fix event decoding - [`c721b1`](https://github.com/use-ink/cargo-contract/commit/c721b19519e579de41217aa347625920925d8040)

## [1.1.0] - 2022-03-18

### Added
- `--skip-linting` flag that allows to skip the linting step during build process - [#468](https://github.com/use-ink/cargo-contract/pull/468)

## [1.0.1] - 2022-03-18
- Improved error reporting during installation - [#469](https://github.com/use-ink/cargo-contract/pull/469)

## [1.0.0] - 2022-03-17

### Changed
- Updated `cargo contract new` template dependencies to ink! `3.0` - [#466](https://github.com/use-ink/cargo-contract/pull/466)

## [0.18.0] - 2022-03-14

### Interact with contracts: upload, instantiate and call commands

We added commands to upload, instantiate and call contracts!
This allows interacting with contracts on live chains with a compatible
[`pallet-contracts`](https://github.com/paritytech/substrate/tree/master/frame/contracts).

For command-line examples on how to use these commands see [#79](https://github.com/use-ink/cargo-contract/pull/79).

### Linting rules for smart contracts

We are introducing a linter for ink! smart contracts in this release!
From now on `cargo-contract` checks if the ink! smart contract that is
`build` or `check`-ed follows certain rules.

As a starting point we've only added one linting rule so far; it asserts correct
initialization of the [`ink_storage::Mapping`](https://use-ink.github.io/ink/ink_storage/struct.Mapping.html)
data structure.

In order for the linting to work with your smart contract, the contract has to be
written in at least ink! 3.0.0-rc9. If it's older the linting will just always succeed.

### Added
- Interact with contracts: upload, instantiate and call commands - [#79](https://github.com/use-ink/cargo-contract/pull/79)
- Add linting to assert correct initialization of [`ink_storage::Mapping`](https://use-ink.github.io/ink/ink_storage/struct.Mapping.html) - [#431](https://github.com/use-ink/cargo-contract/pull/431)

### Changed
- Upgrade `subxt`, SCALE crates, and substrate primitive `sp-*` crates [#451](https://github.com/use-ink/cargo-contract/pull/451).
- Updated `cargo contract new` template dependencies to ink! `3.0.0-rc9` - [#443](https://github.com/use-ink/cargo-contract/pull/443)

## [0.17.0] - 2022-01-19

### Changed
- Updated `cargo contract new` template dependencies to ink! `3.0.0-rc8` - [#402](https://github.com/use-ink/cargo-contract/pull/402)
- Reverted the disabled overflow checks in the `cargo contract new` template - [#376](https://github.com/use-ink/cargo-contract/pull/376)
- Migrated to 2021 edition, enforcing MSRV of `1.56.1` - [#360](https://github.com/use-ink/cargo-contract/pull/360)

### Added
- For contract size optimization added `workspace` section to override parent `workspace` - [#378](https://github.com/use-ink/cargo-contract/pull/378)

## [0.16.0] - 2021-11-25

### Changed
- Updated `cargo contract new` template dependencies to ink! `3.0.0-rc7` - [#374](https://github.com/use-ink/cargo-contract/pull/374)
- Disabled overflow checks in the `cargo contract new` template - [#372](https://github.com/use-ink/cargo-contract/pull/372)
- Use `-Clinker-plugin-lto` if `lto` is enabled (reduces the size of a contract) - [#358](https://github.com/use-ink/cargo-contract/pull/358)
- Deserialize metadata - [#368](https://github.com/use-ink/cargo-contract/pull/368)

### Added
- Added a `--offline` flag to build contracts without network access - [#356](https://github.com/use-ink/cargo-contract/pull/356)

## [0.15.0] - 2021-10-18

### Changed
- Update to `scale-info` 1.0 and support new metadata versioning - [#342](https://github.com/use-ink/cargo-contract/pull/342)
- Update `cargo contract new` template dependencies to ink! `rc6` - [#342](https://github.com/use-ink/cargo-contract/pull/342)

## [0.14.0] - 2021-08-12

### Added
-  Add option for JSON formatted output - [#324](https://github.com/use-ink/cargo-contract/pull/324)

### Changed
- Use new dependency resolver for template contract - [#325](https://github.com/use-ink/cargo-contract/pull/325)
- Do not strip out panic messages in debug builds - [#326](https://github.com/use-ink/cargo-contract/pull/326)

## [0.13.1] - 2021-08-03

### Fixed
- Fixed a Windows issue with contract files in sub-folders - [#313](https://github.com/use-ink/cargo-contract/pull/313)

## [0.13.0] - 2021-07-22

### Added
- Convenient off-chain testing through `cargo contract test` - [#283](https://github.com/use-ink/cargo-contract/pull/283)
- Build contracts in debug mode by default, add `--release` flag - [#298](https://github.com/use-ink/cargo-contract/pull/298)
- Add `--keep-symbols` flag for better Wasm analysis capabilities  - [#302](https://github.com/use-ink/cargo-contract/pull/302)

### Changed
- Change default optimizations pass to focus on code size - [#305](https://github.com/use-ink/cargo-contract/pull/305)

## [0.12.1] - 2021-05-25

### Added
- Suggest `binaryen` installation from GitHub release on outdated version - [#274](https://github.com/use-ink/cargo-contract/pull/274)

### Fixed
- Always use library targets name for contract artifacts - [#277](https://github.com/use-ink/cargo-contract/pull/277)

## [0.12.0] - 2021-04-21

### Fixed
- Fixed `ERROR: The workspace root package should be a workspace member` when building a contract
  under Windows - [#261](https://github.com/use-ink/cargo-contract/pull/261)

### Removed
- Remove support for `--binaryen-as-dependency` - [#251](https://github.com/use-ink/cargo-contract/pull/251)
- Remove support for the deprecated `cargo contract generate-metadata` command - [#265](https://github.com/use-ink/cargo-contract/pull/265)
- Remove pinned `funty` dependency from "new project" template - [#260](https://github.com/use-ink/cargo-contract/pull/260)

## [0.11.1] - 2021-04-06

### Fixed
- Fix `wasm-opt --version` parsing - [#248](https://github.com/use-ink/cargo-contract/pull/248)

## [0.11.0] - 2021-03-31

### Added
- Improve error output for `wasm-opt` interaction - [#244](https://github.com/use-ink/cargo-contract/pull/244)
- Check optimized Wasm output file exists - [#243](https://github.com/use-ink/cargo-contract/pull/243)
- Detect `wasm-opt` version compatibility and improve error messages - [#242](https://github.com/use-ink/cargo-contract/pull/242)
- Detect version mismatches of `parity-scale-codec` in contract and ink! dependency - [#237](https://github.com/use-ink/cargo-contract/pull/237)
- Support specifying `optimization-passes` in the release profile - [#231](https://github.com/use-ink/cargo-contract/pull/231)
- Support specifying `optimization-passes` on the CLI - [#216](https://github.com/use-ink/cargo-contract/pull/216)
- Use `ink::test` attribute in "new project" template - [#190](https://github.com/use-ink/cargo-contract/pull/190)

### Fixed
- Only allow new contract names beginning with an alphabetic character - [#219](https://github.com/use-ink/cargo-contract/pull/219)
- Upgrade `cargo-metadata` and fix usages - [#210](https://github.com/use-ink/cargo-contract/pull/210)

## [0.10.0] - 2021-03-02

### Fixed
- no periods in new contract names - [#192](https://github.com/use-ink/cargo-contract/pull/192)

### Changed
- Update `cargo contract new` template dependencies for `ink!` `rc3` - [#204](https://github.com/use-ink/cargo-contract/pull/204)

## [0.9.1] - 2021-02-24

### Fixed
- Fix linker error when building complex contracts - [#199](https://github.com/use-ink/cargo-contract/pull/199)

## [0.9.0] - 2021-02-22

### Added
- Implement Wasm validation for known issues/markers - [#171](https://github.com/use-ink/cargo-contract/pull/171)

### Changed
- Use either `binaryen-rs` dep or `wasm-opt` binary - [#168](https://github.com/use-ink/cargo-contract/pull/168)
- Update to scale-info 0.5 and codec 2.0 - [#164](https://github.com/use-ink/cargo-contract/pull/164)
- Put build artifacts under `target/ink/` - [#122](https://github.com/use-ink/cargo-contract/pull/122)

### Fixed
- Fix `wasm-opt` regression - [#187](https://github.com/use-ink/cargo-contract/pull/187)
- Generate metadata explicitly for the contract which is build - [#174](https://github.com/use-ink/cargo-contract/pull/174)
- Fix bug with empty Wasm file when using system binaryen for optimization - [#179](https://github.com/use-ink/cargo-contract/pull/179)
- Suppress output on `--quiet` - [#165](https://github.com/use-ink/cargo-contract/pull/165)
- Do not generate build artifacts under `target` for `check` - [#124](https://github.com/use-ink/cargo-contract/pull/124)
- update wasm-path usage name - [#135](https://github.com/use-ink/cargo-contract/pull/135)

## [0.8.0] - 2020-11-27

- Exit with 1 on Err [#109](https://github.com/use-ink/cargo-contract/pull/109)
- Use package name instead of lib name for metadata dependency [#107](https://github.com/use-ink/cargo-contract/pull/107)
- Do not prettify JSON for bundle [#105](https://github.com/use-ink/cargo-contract/pull/105)
- Make `source.hash` non-optional, remove metadata-only [#104](https://github.com/use-ink/cargo-contract/pull/104)
- Implement new commands `build` and `check` + introduce bundles (.contract files) [#97](https://github.com/use-ink/cargo-contract/pull/97)
- Replace xbuild with cargo build-std [#99](https://github.com/use-ink/cargo-contract/pull/99)
- Use binaryen-rs as dep instead of requiring manual wasm-opt installation [#95](https://github.com/use-ink/cargo-contract/pull/95)
- Specify optional --manifest-path for build and generate-metadata [#93](https://github.com/use-ink/cargo-contract/pull/93)

## [0.7.1] - 2020-10-26

- Update new command template to ink! 3.0-rc2 [#85](https://github.com/use-ink/cargo-contract/pull/85)

## [0.7.0] - 2020-10-13

- Fix deprecation warnings [#82](https://github.com/use-ink/cargo-contract/pull/82)
- Use ink 3.0.0-rc1 [#82](https://github.com/use-ink/cargo-contract/pull/82)
- [template] now uses ink_env and ink_storage [#81](https://github.com/use-ink/cargo-contract/pull/81)
- Update new command template to ink! 3.0 syntax [#80](https://github.com/use-ink/cargo-contract/pull/80)
- Extract contract metadata to its own crate [#69](https://github.com/use-ink/cargo-contract/pull/69)
- Fix ManifestPath compiler errors [#73](https://github.com/use-ink/cargo-contract/pull/73)
- Upgrade cargo-xbuild and other dependencies [#71](https://github.com/use-ink/cargo-contract/pull/71)
- Update subxt and async-std dependencies [#66](https://github.com/use-ink/cargo-contract/pull/66)
- Generate extended contract metadata [#62](https://github.com/use-ink/cargo-contract/pull/62)
- Autogenerate abi/metadata package [#58](https://github.com/use-ink/cargo-contract/pull/58)
- Extract workspace to module directory [#59](https://github.com/use-ink/cargo-contract/pull/59)
- Add preferred default release profile settings [#55](https://github.com/use-ink/cargo-contract/pull/55)
- Add option to build with unmodified original manifest [#51](https://github.com/use-ink/cargo-contract/pull/51)
- Update cargo-xbuild [#54](https://github.com/use-ink/cargo-contract/pull/54)

## [0.6.1] - 2020-05-12

- Fix LTO regressions in nightly toolchain [#52](https://github.com/use-ink/cargo-contract/pull/52)

## [0.6.0] - 2020-03-25

- First release to crates.io
- Use `subxt` release from [crates.io](https://crates.io/crates/substrate-subxt)

## [0.5.0] - 2020-03-18

- Upgrades dependencies [#45](https://github.com/use-ink/cargo-contract/pull/45)
- Update template to ink! 2.0 dependencies [#47](https://github.com/use-ink/cargo-contract/pull/47)

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

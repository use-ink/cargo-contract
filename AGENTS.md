# Repository Guidelines

## Project Structure & Module Organization
- `crates/`: Rust workspace crates. Key ones include `cargo-contract` (CLI), `build` (build pipeline), `extrinsics` (chain interactions), `metadata`, `transcode`, and `analyze`.
- `docs/`: Command and feature documentation (e.g., RPC usage).
- `build-image/`: Docker image and verifiable-build tooling.
- Tests live alongside code in each crate (`src/**` with `#[cfg(test)]`) plus integration tests in `crates/cargo-contract/tests/` and `crates/extrinsics/src/integration_tests.rs`.

## Build, Test, and Development Commands
- `cargo check --manifest-path crates/<crate>/Cargo.toml`: Per-crate compile check (matches CI behavior).
- `cargo +nightly fmt --all -- --check`: Formatting verification (CI uses nightly rustfmt).
- `cargo +nightly clippy --all-features --all-targets -- -D warnings`: Linting with all features.
- `cargo test --workspace --all-features`: Run the full workspace test suite.
- `cargo nextest run --workspace --all-features`: Faster test runner (used in CI if installed).

## Coding Style & Naming Conventions
- Formatting is enforced by rustfmt (nightly). Prefer standard Rust conventions: 4-space indentation, `snake_case` for functions/modules, `CamelCase` for types, and `SCREAMING_SNAKE_CASE` for constants.
- Clippy is treated as warnings-as-errors in CI. Fix or explicitly justify any lint deviations.

## Testing Guidelines
- Unit tests are colocated with code; integration tests are separated under `crates/cargo-contract/tests/` and `crates/extrinsics/src/integration_tests.rs`.
- Integration tests are feature-gated. Example: `cargo test -p contract-extrinsics --features integration-tests`.
- Add regression tests for fixes and keep test names descriptive of the behavior under test.

## Commit & Pull Request Guidelines
- Commit messages commonly follow a lightweight conventional style: `feat:`, `fix:`, `chore(deps):`, `linting:`, etc. Use this format when possible.
- PRs should follow `.github/pull_request_template.md`, including a changelog entry in `CHANGELOG.md`, test coverage for changes, and notes on breaking changes or ink!/pallet-contracts dependencies.

## Security & Configuration Tips
- For security issues, follow `SECURITY.md`.
- For deterministic builds, prefer the Docker-based verifiable build flow described in `build-image/`.

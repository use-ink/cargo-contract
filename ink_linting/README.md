# ink! linting rules

This crate uses [`dylint`](https://github.com/trailofbits/dylint) to define custom
linting rules for [ink!](https://github.com/paritytech/ink);

You can use it this way:

```bash
# Install all prerequisites.
cargo install cargo-dylint dylint-link

cargo build --release

# Run the linting on a contract.
DYLINT_LIBRARY_PATH=$PWD/target/release cargo dylint contract_instantiated
    --manifest-path ../ink/examples/erc20/Cargo.toml 
```
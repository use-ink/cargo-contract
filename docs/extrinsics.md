# Extrinsics
____
`cargo-contract` provides CLI support for uploading, instantiating and calling your contracts directly from the command 
line.

## Common arguments

```
--suri
```
The Secret URI used for signing the extrinsic. For development chains, the well known endowed accounts can be used e.g.
`//Alice`. For other accounts, the actual secret key must be provided e.g. an `0x` prefixed 64 bit hex string, or the
seed phrase. See usage of [`subkey`](https://docs.substrate.io/v3/tools/subkey/) for examples, and docs for the expected
values in the [parsing code](https://docs.rs/sp-core/latest/sp_core/crypto/trait.Pair.html#method.from_string_with_seed).

:warning: **IMPORTANT** :warning:

It is strongly recommended NOT to use secret keys from actual value bearing chains on the command line, since they are
visible on screen and are often saved to the command line shell's history. For now this tool should only be used for
development and testnets. It is a priority to implement a safer method of signing here before using this tool with value
bearing chains.

```
--password
```
*Optional*. The password for the `--suri`, see https://docs.substrate.io/v3/tools/subkey/#password-protected-keys.

```
--manfest-path
```
*Optional*. The path to the `Cargo.toml` of the contract crate. Use this to run commands on a contract from outside of 
its project directory.

```
--url
```
*Optional*. The websockets url of an RPC node on the target chain. Defaults to a locally running node at 
"ws://localhost:9944".

```
---dry-run
```
*Optional*. All extrinsic commands can be run without altering the chain state. Useful for testing if a command will be
successful, estimating gas costs or querying the result of `ink!` readonly messages.

## Commands

### `upload`

### `instantiate`

### `call`



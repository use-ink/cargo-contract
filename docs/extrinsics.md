# Extrinsics
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

:warning: **WARNING** :warning:

It is strongly recommended NOT to use secret keys from actual value bearing chains on the command line, since they are
visible on screen and are often saved to the command line shell's history. For now this tool should only be used for
development and testnets. It is a priority to implement a safer method of signing here before using this tool with value
bearing chains.

```
--password
```
*Optional*. The password for the `--suri`, see https://docs.substrate.io/v3/tools/subkey/#password-protected-keys.

```
--manifest-path
```
*Optional*. The path to the `Cargo.toml` of the contract crate. Use this to run commands on a contract from outside of
its project directory.

```
--url
```
*Optional*. The websockets url of an RPC node on the target chain. Defaults to a locally running node at
"ws://localhost:9944".

```
-x/--execute
```
*Optional*. All extrinsic commands run without altering the chain state by default. This flag specifies
that the extrinsic needs to be executed on chain.

```
--storage-deposit-limit
```
*Optional*. The maximum amount of balance that can be charged from the caller to pay for the storage consumed.

## Commands

### `upload`

Upload the Wasm code of the contract to the target chain. Invokes the [`upload_code`](https://github.com/paritytech/substrate/blob/master/frame/contracts/src/lib.rs#L509)
dispatchable.

e.g. `cargo contract upload --suri //Alice`

Assumes that `cargo contract build` has already been run to produce the contract artifacts.

### `instantiate`

Create an instance of a contract on chain. If the code has already been uploaded via `upload`, specify the resulting
`--code-hash` which will result in a call to [`instantiate`](https://github.com/paritytech/substrate/blob/master/frame/contracts/src/lib.rs#L460).
If no `--code-hash` is specified it will attempt to both upload the code and instantiate via the
[`instantiate_with_code`](https://github.com/paritytech/substrate/blob/master/frame/contracts/src/lib.rs#L419)
dispatchable.

e.g.
```
cargo contract instantiate \
       --constructor new \
       --args false \
       --suri //Alice \
       --code-hash 0xbc1b42256696c8a4187ec3ed79fc602789fc11287c4c30926f5e31ed8169574e
```
- `--constructor` the name of the contract constructor method to invoke.
- `--args` accepts a space separated list of values, encoded in order as the arguments of the constructor to invoke.
- `--code-hash` the hash of the uploaded code, returned from a call to `contract upload` or a previous
`contract instantiate`

### `call`

Invoke a message on an instance of a contract via the [`call`](https://github.com/paritytech/substrate/blob/master/frame/contracts/src/lib.rs#L359)
dispatchable.

e.g.
```
cargo contract call \
       --contract 5FKy7RwXBCCACCEPjM5WugkhUd787FjdgieTkdj7TPngJzxN \
       --message transfer \
       --args 5FKy7RwXBCCACCEPjM5WugkhUd787FjdgieTkdj7TPngJzxN 1000 \
       --suri //Alice
```

- `--contract` the account id of the contract to invoke, returned after a successful `contract instantiate`.
- `--message` the name of the contract message to invoke.
- `--args` accepts a space separated list of values, encoded in order as the arguments of the message to invoke.

### `remove`

Remove the Wasm code of the contract to the target chain. Invokes the [`remove_code`](https://github.com/paritytech/substrate/blob/master/frame/contracts/src/lib.rs#L581)
dispatchable.

e.g. `cargo contract remove --suri //Alice`

Assumes that `cargo contract build` and `cargo contract upload` have already been run to produce the contract artifacts.
This command will only succeed if there are no contract instances of this code. Contracts which have already been instantiated from this code must either `terminate` themselves or have their code changed via a `set_code` call to `pallet_contracts`.

```
cargo contract remove \
       --suri //Alice \
       --code-hash 0xbc1b42256696c8a4187ec3ed79fc602789fc11287c4c30926f5e31ed8169574e
```

- `--code-hash` the hash of the uploaded code, returned from a call to `contract upload`.
If not specified the code hash will be taken from the contract artifacts.

## Specifying the contract artifact

The above examples assume the working directory is the contract source code where the `Cargo.toml` file is located.
This is used to determine the location of the contract artifacts. Alternatively, there is an optional positional
argument to each of the extrinsic commands which allows specifying the contract artifact file directly. E.g.

- `cargo upload ../path/to/mycontract.wasm`
- `cargo instantiate ../path/to/mycontract.contract`
- `cargo call ..path/to/mycontract.json`






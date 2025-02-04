# Other commands
`cargo-contract` provides CLI support for displaying info directly from the command
line.

### `info`

Fetch and display information for a given contract.

e.g.

```
cargo contract info \
      --contract 5DVGLfDGBvqMr9nCg48g99oD8Mz3sruWmb6ek5UbWvDnbTgZ
```

- `--contract` the account id of the instantiated contract to examine.

*Optional*
- `--url` the url of the rpc endpoint you want to specify - by default `ws://localhost:9944`.
- `--config` the chain config to be used as part of the call - by default `Polkadot`.
- `--chain` the name of a production chain to be communicated with, conflicts with `--url` and `--config`
- `--output-json` to export the output as JSON.
- `--binary` outputs the contract as a binary blob. If used in combination with `--output-json`, outputs the contract's binary as a JSON object with hex string.
- `--all` outputs all contracts addresses. It can not be used together with `--binary` flag.

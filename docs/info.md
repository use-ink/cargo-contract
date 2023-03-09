# Other commands
`cargo-contract` provides CLI support for displaying info directly from the command
line.

### `info`

Fetch and display information for a given contract.

e.g. `cargo contract info --contract 5DVGLfDGBvqMr9nCg48g99oD8Mz3sruWmb6ek5UbWvDnbTgZ`

Assumes that `cargo contract build`, `cargo contract upload` and `cargo contract instantiate` have already been run to display information for the contract.

```
cargo contract info \
      --contract 5DVGLfDGBvqMr9nCg48g99oD8Mz3sruWmb6ek5UbWvDnbTgZ
```

- `--contract` the account id of the contract to examine.

*Optional*
- `--url` the url of the rpc endpoint you want to specify - by default `ws://localhost:9944`.
- `--output-json` to export the output as JSON.

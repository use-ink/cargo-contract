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
- `--output-json` to export the output as JSON.
- `--binary` to display the pristine Wasm code.

# Other commands
`cargo-contract` provides CLI support for displaying info directly from the command
line.

### `info`

Fetch and display informations for a given contractId (AccountId). 
Display these information (https://github.com/paritytech/substrate/blob/master/frame/contracts/src/storage.rs#L45-L69) from 


e.g. `cargo contract info --suri //Alice`

Assumes that `cargo contract build`, `cargo contract upload` and `cargo contract instantiate` have already been run to produce the contract artifacts.

```
cargo contract info \
       --contract 5DVGLfDGBvqMr9nCg48g99oD8Mz3sruWmb6ek5UbWvDnbTgZ
```

- `--contract` the account id of the contract to invoke, returned after a successful `contract instantiate`

*Optional*
- `--url` the url of the rpc endpoint you want to specify - by default ws://localhost:9944 .

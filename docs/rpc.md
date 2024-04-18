### `rpc`

Invoke an RPC call to the node in the format:
`cargo contract rpc [Options] METHOD [PARAMS]`

e.g.

```bash
cargo contract rpc author_insertKey '"sr25"' '"//ALICE"' \
      5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY

 cargo contract rpc author_hasKey
     5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY '"sr25"' \
```
 
account can be provided as ss58 address:
`5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY`
or in hex:
`0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d`

Some of the commands require SS58 address in the string format:

 ```bash
cargo contract rpc system_accountNextIndex \
      '"5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY"'
```

Command using a sequence as a parameter:

```bash
cargo contract rpc state_getReadProof \
      '(0x4b9cce91a924c0f4d469b3d62e02f9682079560c6cfc45c1a9498812dfff4b3a)'
```

*Optional*

- `--url` the url of the rpc endpoint you want to specify - by default `ws://localhost:9944`.
- `--config` the chain config to be used as part of the call - by default `Polkadot`.
- `--chain` the name of a production chain to be communicated with, conflicts with `--url` and `--config`.
- `--output-json` to export the output as JSON.

#!/bin/bash

# Compiles Solidity smart contract to WASM using Solang `solang`

trap "echo; exit" INT
trap "echo; exit" HUP

SOLIDITY_FILENAME=$1
SOLIDITY_FILE_PATH=$2

if ! command -v solang &> /dev/null
then
    echo "solang command could not be found.\n\n"
    echo "Please follow the installation instructions at https://github.com/hyperledger/solang then try again..."
else
    echo "Detected solang binary...\n"
    echo "Building ${SOLIDITY_FILENAME} using Solang Compiler for Substrate.\n"
    echo "Generating ABI .contract and contract .wasm files.\n"

    # example: https://solang.readthedocs.io/en/latest/examples.html#flipper
    solang compile $SOLIDITY_FILE_PATH
fi

exit

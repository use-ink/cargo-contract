#!/bin/bash

trap "echo; exit" INT
trap "echo; exit" HUP

SOLIDITY_FILENAME=$1
SOLIDITY_FILE_PATH=$2
SOLANG_TARGET=$3

if ! command -v solang &> /dev/null
then
    echo "solang command could not be found.\n\n"
    echo "Please follow the installation instructions at https://github.com/hyperledger/solang then try again..."
    exit
fi

echo "Detected solang binary...\n"
echo "Building ${SOLIDITY_FILENAME} using Solang for target ${SOLANG_TARGET}.\n"
echo "Generating ABI .contract and contract .wasm files.\n"

# example: https://solang.readthedocs.io/en/latest/examples.html#flipper
solang compile --target $SOLANG_TARGET $SOLIDITY_FILE_PATH
exit

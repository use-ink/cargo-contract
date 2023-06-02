#!/bin/bash

# Compiles Solidity smart contract to either WASM using Solang `solang` or EVM bytecode using Solidity Compiler `solc`

trap "echo; exit" INT
trap "echo; exit" HUP

SOLIDITY_FILENAME=$1
SOLIDITY_FILE_PATH=$2
SOLANG_TARGET=$3
COMPILE_TO=$4
TARGET_EVM_VERSION=$5

if [[ $COMPILE_TO == "evm" ]]
then
    if ! command -v solc &> /dev/null
    then
        echo "solc command could not be found.\n\n"
        echo "Please follow the installation instructions at https://docs.soliditylang.org/ to install the Solidity Compiler in your PATH then try again..."
    else
        echo "Detected solc binary...\n"
        echo "Building ${SOLIDITY_FILENAME} using Solidity Compiler for target ${COMPILE_TO}.\n"

        solc --evm-version $TARGET_EVM_VERSION $SOLIDITY_FILE_PATH
    fi

    if ! command -v solcjs &> /dev/null
    then
        echo "solcjs command could not be found.\n\n"
        echo "Please follow the installation instructions at https://docs.soliditylang.org/ to install the Solidity Compiler in your PATH then try again..."
    else
        echo "Detected solcjs binary...\n"
        echo "Building ${SOLIDITY_FILENAME} using Solidity Compiler for target ${COMPILE_TO}.\n"
        echo "Generating ABI .abi file and binary .bin file.\n"

        solcjs --abi --bin $SOLIDITY_FILE_PATH
    fi
# COMPILE_TO must be "wasm"
else
    if ! command -v solang &> /dev/null
    then
        echo "solang command could not be found.\n\n"
        echo "Please follow the installation instructions at https://github.com/hyperledger/solang then try again..."
    else
        echo "Detected solang binary...\n"
        echo "Building ${SOLIDITY_FILENAME} using Solang Compiler for target ${SOLANG_TARGET}.\n"
        echo "Generating ABI .contract and contract .wasm files.\n"

        # example: https://solang.readthedocs.io/en/latest/examples.html#flipper
        solang compile --target $SOLANG_TARGET $SOLIDITY_FILE_PATH
    fi
fi

exit

ARG VCS_REF=master
ARG REGISTRY_PATH=docker.io/paritytech
ARG BUILD_DATE

FROM ${REGISTRY_PATH}/base-ci-linux:latest

# metadata
LABEL io.parity.image.vendor="Parity Technologies" \
    io.parity.image.title="${REGISTRY_PATH}/contracts-verified" \
    io.parity.image.documentation="https://github.com/paritytech/scripts/blob/${VCS_REF}/\
    dockerfiles/ink-ci-linux/README.md" \
    io.parity.image.description="Inherits from docker.io/paritytech/base-ci-linux. \
    rust nightly, clippy, rustfmt, miri, rust-src, rustc-dev, grcov, rust-covfix, \
    llvm-tools-preview, cargo-contract, xargo, binaryen, parallel, codecov, ink, solang" \
    io.parity.image.revision="${VCS_REF}" \
    io.parity.image.documentation="https://github.com/paritytech/cargo-contract/blob/${VCS_REF}/\
    build-image/README.md" \
    io.parity.image.created="${BUILD_DATE}"

WORKDIR /builds

RUN set -eux; \
    apt-get -y update && \
    apt-get install -y --no-install-recommends zlib1g-dev npm wabt && \
    npm install --ignore-scripts -g yarn && \

    # `binaryen` is needed by `cargo-contract` for optimizing Wasm files.
    # We fetch the latest release which contains a Linux binary.
    curl -L $(curl --silent https://api.github.com/repos/WebAssembly/binaryen/releases \
    | jq -r '.[0].assets | [.[] | .browser_download_url] | map(select(match("x86_64-linux\\.tar\\.gz$"))) | .[0]' \
    ) | tar -xz -C /usr/local/bin/ --wildcards --strip-components=2 'binaryen-*/bin/wasm-opt' && \

    # Install LLVM for solang
    curl -L https://github.com/hyperledger/solang-llvm/releases/download/llvm15-1/llvm15.0-linux-x86-64.tar.xz > llvm15.0-linux-x86-64.tar.xz && \
    tar Jxf llvm15.0-linux-x86-64.tar.xz && \
    export PATH=$(pwd)/llvm15.0/bin:$PATH && \

    # The stable toolchain is used to build ink! itself through the use of the
    # `RUSTC_BOOSTRAP=1` environment variable. We also need to install the
    # `wasm32-unknown-unknown` target since that's the platform that ink! smart contracts
    # run on.

    # The 1.69 toolchain is temporarily required to build ink! contracts because of
    # https://github.com/paritytech/cargo-contract/issues/1139 \
    rustup toolchain install 1.69 && \
    rustup target add wasm32-unknown-unknown --toolchain 1.69 && \
    rustup component add rust-src clippy rustfmt --toolchain 1.69 && \
    rustup default 1.69 && \

    # `cargo-dylint` and `dylint-link` are dependencies needed to run `cargo-contract`.
    cargo install cargo-dylint dylint-link && \

    # Install the latest stable release of `cargo-contract`
    cargo install cargo-contract && \

    # Install the latest `solang`
    cargo install solang && \

    # Versions
    rustup show && \
    cargo --version && \
    cargo-contract --version && \
    wasm-opt --version && \

    # Clean up and remove compilation artifacts that a cargo install creates (>250M).
    rm -rf "${CARGO_HOME}/registry" "${CARGO_HOME}/git" /root/.cache/sccache && \

    # apt clean up
    apt-get remove -y gnupg && \
    apt-get autoremove -y && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

# A contract workdir which should be bind mounted to the container. See README for detailed documentation.
WORKDIR /contract

# A default entry point for manual execution
ENTRYPOINT [ "cargo", "contract", "build", "--release" ]

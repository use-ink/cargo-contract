# The rust version to use
# The 1.69 toolchain is temporarily required to build ink! contracts because of
# https://github.com/paritytech/cargo-contract/issues/1139
ARG RUST_VERSION=1.69
# The cargo contract version to use
ARG CARGO_CONTRACT_VERSION=3.0.1

FROM docker.io/bitnami/minideb:bullseye-amd64 as slimmed-rust

# metadata
LABEL io.parity.image.vendor="Parity Technologies" \
    io.parity.image.title="contracts-verifiable" \
    io.parity.image.description="Inherits from docker.io/bitnami/minideb:bullseye. \
    rust nightly, clippy, rustfmt, miri, rust-src, rustc-dev, grcov, rust-covfix, \
    llvm-tools-preview, cargo-contract, xargo, binaryen, parallel, codecov, ink, solang" \
    io.parity.image.documentation="https://github.com/paritytech/cargo-contract/blob/master/\
    build-image/README.md" \
    io.parity.version.rust=${RUST_VERSION} \
    io.parity.version.cargo-contract=${CARGO_CONTRACT_VERSION}

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# Minimal Rust dependencies.
RUN set -eux \
    && apt-get update && apt-get -y install wget \
    && url="https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init" \
    && wget "$url" \
    && chmod +x rustup-init \
    && ./rustup-init -y --no-modify-path --profile minimal --component rust-src rustfmt --default-toolchain $RUST_VERSION  \
    && rm rustup-init \
    && chmod -R a+w $RUSTUP_HOME $CARGO_HOME \
    && rustup --version \
    && cargo --version \
    && rustc --version \
    && apt-get remove -y --auto-remove wget \
    && apt-get -y install gcc \
    && rm -rf /var/lib/apt/lists/*

FROM slimmed-rust as cc-builder

# This is important, see https://github.com/rust-lang/docker-rust/issues/85
ENV RUSTFLAGS="-C target-feature=-crt-static"

    # Install required packages for `cargo-contract`
RUN apt-get -y update && apt-get -y install gcc g++ \
    # Install cargo contract
    && cargo install cargo-contract --locked --version $CARGO_CONTRACT_VERSION \
    # Cleanup after `cargo install`
    && rm -rf ${CARGO_HOME}/"registry" ${CARGO_HOME}/"git" /root/.cache/sccache \
    # apt clean up
    && apt-get remove -y gnupg \
    && apt-get autoremove -y \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*


FROM slimmed-rust as ink-dev

COPY --from=cc-builder /usr/local/cargo/bin/cargo-contract /usr/local/bin/cargo-contract

WORKDIR /contract

# default entry point
ENTRYPOINT ["cargo", "contract", "build", "--release"]

FROM docker.io/bitnami/minideb:bullseye-amd64 as slimmed-rust

# The rust version to use
# The 1.69 toolchain is temporarily required to build ink! contracts because of
# https://github.com/paritytech/cargo-contract/issues/1139
ARG RUST_VERSION=1.69
# The cargo contract version to use
ARG CARGO_CONTRACT_VERSION=3.0.1
# gcc package version
ARG GCC_VERSION=4:10.2.1-1
# wget package version
ARG WGET_VERSION=1.21-1+deb11u1
# g++ package version
ARG G_VERSION=4:10.2.1-1
# Url to the cargo-contract repository to install from
ARG CARGO_CONTRACT_GIT

# metadata
LABEL io.parity.image.vendor="Parity Technologies" \
    io.parity.image.title="paritytech/contracts-verifiable" \
    io.parity.image.description="Inherits from docker.io/bitnami/minideb:bullseye. \
    rust nightly, clippy, rustfmt, miri, rust-src, rustc-dev, grcov, rust-covfix, \
    llvm-tools-preview, cargo-contract, xargo, binaryen, parallel, codecov, ink, solang" \
    io.parity.image.documentation="https://github.com/paritytech/cargo-contract/blob/master/\
    build-image/README.md" \
    io.parity.version.rust=${RUST_VERSION} \
    io.parity.version.cargo-contract=${CARGO_CONTRACT_VERSION} \
    io.parity.version.gcc=${GCC_VERSION} \
    io.parity.version.wget=${WGET_VERSION} \
    io.parity.version.g_plus_plus=${G_VERSION}

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# Minimal Rust dependencies.
RUN set -eux \
    && apt-get update && apt-get -y install wget=${WGET_VERSION} \
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
    && apt-get -y install gcc=${GCC_VERSION} \
    && rm -rf /var/lib/apt/lists/*

FROM slimmed-rust as cc-builder
ARG CARGO_CONTRACT_VERSION
ARG GCC_VERSION
ARG G_VERSION
ARG CARGO_CONTRACT_GIT


# This is important, see https://github.com/rust-lang/docker-rust/issues/85
ENV RUSTFLAGS="-C target-feature=-crt-static"

    # Install required packages for `cargo-contract`
RUN apt-get -y update && apt-get -y install gcc=${GCC_VERSION} g++=${G_VERSION} \
    # Install cargo contract from git if the arg is set
    && if [ -n "$CARGO_CONTRACT_GIT" ]; then \
    cargo install --git ${CARGO_CONTRACT_GIT} ;\
    else \
    # Otherwise install from crates.io
    cargo install cargo-contract --locked --version ${CARGO_CONTRACT_VERSION} ;\
    fi \
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

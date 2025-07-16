FROM debian:stable-slim AS slimmed-rust

# The rust version to use
ARG RUST_VERSION=stable
# The cargo contract version to use
ARG CARGO_CONTRACT_VERSION=6.0.0-alpha
# Url to the cargo-contract repository to install from
ARG CARGO_CONTRACT_GIT
# Branch to use in git repository
ARG CARGO_CONTRACT_BRANCH
# Commit to use in git repository
ARG CARGO_CONTRACT_REV
# Tag to use in git repository
ARG CARGO_CONTRACT_TAG
# gcc package version
ARG GCC_VERSION=4:12.2.0-3
# wget package version
ARG WGET_VERSION=1.21.3-1+deb12u1
# g++ package version
ARG G_VERSION=4:12.2.0-3
ARG MUSL_V=1.2.3-1
# The Rust version used by `ink_linting`.
# See https://github.com/use-ink/ink/blob/master/linting/rust-toolchain.toml.
ARG RUST_LINTER_VERSION=nightly-2025-05-14
ARG TARGETARCH

# Metadata
LABEL ink.use.image.vendor="Use Ink" \
    ink.use.image.title="useink/contracts-verifiable" \
    ink.use.image.description="Inherits from debian:stable-slim. \
    rust nightly, clippy, rustfmt, miri, rust-src, rustc-dev, grcov, rust-covfix, \
    llvm-tools-preview, cargo-contract, xargo, binaryen, parallel, codecov, ink, solang" \
    ink.use.image.documentation="https://github.com/use-ink/cargo-contract/blob/master/\
    build-image/README.md" \
    ink.use.version.rust=${RUST_VERSION} \
    ink.use.version.cargo-contract.version=${CARGO_CONTRACT_VERSION} \
    ink.use.version.cargo-contract.git.repository=${CARGO_CONTRACT_GIT} \
    ink.use.version.cargo-contract.git.branch=${CARGO_CONTRACT_BRANCH} \
    ink.use.version.cargo-contract.git.revision=${CARGO_CONTRACT_REV} \
    ink.use.version.cargo-contract.git.tag=${CARGO_CONTRACT_TAG} \
    ink.use.version.gcc=${GCC_VERSION} \
    ink.use.version.wget=${WGET_VERSION} \
    ink.use.version.g_plus_plus=${G_VERSION} \
    ink.use.version.musl=${MUSL_V} \
    ink.use.version.rust_linter=${RUST_LINTER_VERSION}

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    ARCH=$TARGETARCH

# Minimal Rust dependencies.
RUN set -eux \
    && apt-get update && apt-get -y install wget=${WGET_VERSION} gcc=${GCC_VERSION} \
    # Map target architecture to Rust format
    # NOTE: `arm64` support is for debugging this `Dockerfile` locally only! No `arm64` images are to be published.
    && case ${TARGETARCH} in \
            "amd64")  RUST_ARCH="x86_64"  ;; \
            "arm64")  RUST_ARCH="aarch64" ;; \
            *)        RUST_ARCH=${TARGETARCH} ;; \
    esac \
    && url="https://static.rust-lang.org/rustup/dist/${RUST_ARCH}-unknown-linux-gnu/rustup-init" \
    && wget "$url" \
    && chmod +x rustup-init \
    && ./rustup-init -y --no-modify-path --profile minimal --component rust-src,rustfmt,clippy --default-toolchain $RUST_VERSION \
    && rm rustup-init \
    # Install nightly toolchain required by `cargo contract lint --extra-lints` command
    && rustup install ${RUST_LINTER_VERSION} --profile minimal \
    && rustup component add rust-src --toolchain ${RUST_LINTER_VERSION} \
    && chmod -R a+w $RUSTUP_HOME $CARGO_HOME \
    && rustup --version \
    && cargo --version \
    && rustc --version \
    # Clean up layer
    && apt-get remove -y --auto-remove wget \
    && apt-get clean \
    && apt-get autoclean \
    && apt-get autoremove -y \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/* /var/cache/apt/archives/* \
        /usr/share/doc/* /usr/share/man/* /usr/share/locale/* /var/log/* /var/cache/debconf/*

FROM slimmed-rust AS cc-builder
ARG CARGO_CONTRACT_VERSION
ARG GCC_VERSION
ARG G_VERSION
ARG MUSL_V
ARG CARGO_CONTRACT_BRANCH
ARG CARGO_CONTRACT_TAG
ARG CARGO_CONTRACT_REV
ARG CARGO_CONTRACT_GIT
ARG RUST_LINTER_VERSION

# This is important, see https://github.com/rust-lang/docker-rust/issues/85
ENV DYLINT_DRIVER_PATH=/usr/local/dylint_drivers

# Install required packages for `cargo-contract`
# libssl-dev and pkg-config is required by cargo-dylint
RUN apt-get -y update && apt-get -y install gcc=${GCC_VERSION} g++=${G_VERSION} musl-dev=${MUSL_V} libssl-dev pkg-config \
    # Install cargo contract from git if the arg is set
    && if [ -n "$CARGO_CONTRACT_GIT" ]; then \
    COMMAND="cargo install --git ${CARGO_CONTRACT_GIT}" ; \
    else \
    COMMAND="cargo install cargo-contract --locked --version ${CARGO_CONTRACT_VERSION}" ;\
    fi \
    && if [ -n "$CARGO_CONTRACT_BRANCH" ] && [ -n "$CARGO_CONTRACT_GIT" ]; then \
    COMMAND="${COMMAND} --branch ${CARGO_CONTRACT_BRANCH}" ; \
    fi \
    && if [ -n "$CARGO_CONTRACT_REV" ] && [ -n "$CARGO_CONTRACT_GIT" ]; then \
    COMMAND="${COMMAND} --rev ${CARGO_CONTRACT_REV}" ; \
    fi \
    && if [ -n "$CARGO_CONTRACT_TAG" ] && [ -n "$CARGO_CONTRACT_GIT" ]; then \
    COMMAND="cargo install --git ${CARGO_CONTRACT_GIT} --tag ${CARGO_CONTRACT_TAG}" ; \
    fi \
    && echo "Executing ${COMMAND}" \
    && eval "${COMMAND}" \
    # Install cargo-dylint, dylint-link required by `cargo contract lint --extra-lints` command
    && cargo +${RUST_LINTER_VERSION} install cargo-dylint dylint-link \
    # Check if build and lint works
    && mkdir -p $DYLINT_DRIVER_PATH \
    && cargo contract new test \
    # Generate /usr/local/dylint_drivers/${RUST_LINTER_VERSION}-{ARCH}-unknown-linux-gnu/dylint-driver binary
    && cd test && cargo contract build --verbose --release && cargo contract lint --extra-lints && cd ..

FROM slimmed-rust AS ink-dev

COPY --from=cc-builder /usr/local/dylint_drivers /usr/local/dylint_drivers
COPY --from=cc-builder /usr/local/cargo/bin/cargo-contract /usr/local/bin/cargo-contract
COPY --from=cc-builder /usr/local/cargo/bin/cargo-dylint /usr/local/cargo/bin/cargo-dylint
COPY --from=cc-builder /usr/local/cargo/bin/dylint-link /usr/local/cargo/bin/dylint-link

WORKDIR /contract
ENV DYLINT_DRIVER_PATH=/usr/local/dylint_drivers

# default entry point
ENTRYPOINT ["cargo", "contract"]

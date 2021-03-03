FROM ubuntu:focal as builder
LABEL maintainer "who_should_this_be@gmail.com"
LABEL description="This image contains the cargo contract command"

ARG RUSTC_VERSION="nightly-2020-10-27"
ENV RUSTC_VERSION=$RUSTC_VERSION
ENV PROFILE=release
ENV PACKAGE=polkadot-runtime

RUN mkdir -p /cargo-home /rustup-home
WORKDIR /build
ENV RUSTUP_HOME="/rustup-home"
ENV CARGO_HOME="/cargo-home"
ENV DEBIAN_FRONTEND="noninteractive"
ENV PATH="/cargo-home/bin:$PATH"

# We first init as much as we can in the first layers
RUN apt-get update && \
    apt-get upgrade -y && \
    apt-get install --no-install-recommends -y \
        cmake pkg-config libssl-dev \
        git clang bsdmainutils jq ca-certificates curl binaryen && \
    curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain $RUSTC_VERSION -y &&\
    rm -rf /var/lib/apt/lists/* &&\
    rustup toolchain install nightly --target wasm32-unknown-unknown \
    --profile minimal --component rustfmt rust-src; \
    rustup default nightly &&\
    cargo install --git https://github.com/paritytech/cargo-contract.git --force

#RUN cargo contract new test

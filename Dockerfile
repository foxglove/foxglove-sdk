# Local development image. See Makefile for usage.
FROM rust:1.89-trixie AS builder

ARG MSRV_RUST_VERSION=1.85.0

WORKDIR /app

RUN rustup toolchain install nightly --component rust-src
RUN rustup toolchain install ${MSRV_RUST_VERSION}
RUN rustup component add rustfmt clippy

RUN apt-get update \
    && apt-get install -y \
        clang-19 \
        clang-format-19 \
        clang-tidy-19 \
        cmake \
        doxygen \
        libva-dev \
        nodejs \
        npm \
        protobuf-compiler \
        python3-dev \
    && rm -rf /var/lib/apt/lists/*

RUN corepack enable yarn

ENV PATH=/usr/lib/llvm-19/bin:/root/.local/bin:$PATH \
    COREPACK_ENABLE_DOWNLOAD_PROMPT=0

RUN curl -LsSf https://astral.sh/uv/install.sh | sh

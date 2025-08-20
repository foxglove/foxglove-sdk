FROM rust:latest AS builder

ARG MSRV_RUST_VERSION=1.83.0

WORKDIR /app

RUN rustup toolchain install nightly --component rust-src
RUN rustup toolchain install ${MSRV_RUST_VERSION}
RUN rustup component add rustfmt clippy

RUN curl -fsSL https://deb.nodesource.com/setup_23.x -o nodesource_setup.sh
RUN bash nodesource_setup.sh

RUN apt-get update \
    && apt-get install -y \
        clang-19 \
        clang-format-19 \
        clang-tidy-19 \
        cmake \
        doxygen \
        nodejs \
        protobuf-compiler \
        python3.13-dev \
    && rm -rf /var/lib/apt/lists/*

RUN corepack enable yarn

ENV PATH=/usr/lib/llvm-19/bin:$PATH \
    POETRY_NO_INTERACTION=1 \
    POETRY_CACHE_DIR='/var/cache/pypoetry' \
    POETRY_HOME='/usr/local' \
    COREPACK_ENABLE_DOWNLOAD_PROMPT=0

RUN curl -sSL https://install.python-poetry.org | python3 -

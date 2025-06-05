FROM rust:latest AS builder

ARG MSRV_RUST_VERSION=1.83.0

WORKDIR /app

RUN rustup toolchain install nightly
RUN rustup toolchain install ${MSRV_RUST_VERSION}
RUN rustup component add rustfmt clippy

RUN curl -fsSL https://deb.nodesource.com/setup_23.x -o nodesource_setup.sh
RUN bash nodesource_setup.sh

RUN apt-get update \
    && apt-get install -y \
        protobuf-compiler=3.21.12-3 \
        python3.11-dev=3.11.2-6+deb12u6 \
        clang-format-19=1:19.1.4-1~deb12u1 \
        nodejs=23.11.1-1nodesource1 \
    && rm -rf /var/lib/apt/lists/*

RUN ln -s /usr/bin/clang-format-19 /usr/bin/clang-format

RUN corepack enable yarn

ENV POETRY_NO_INTERACTION=1 \
    POETRY_CACHE_DIR='/var/cache/pypoetry' \
    POETRY_HOME='/usr/local' \
    COREPACK_ENABLE_DOWNLOAD_PROMPT=0

RUN curl -sSL https://install.python-poetry.org | python3 -

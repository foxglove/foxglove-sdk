FROM rust:1.83.0 AS builder

WORKDIR /app

RUN rustup component add rustfmt clippy

RUN curl -fsSL https://deb.nodesource.com/setup_23.x -o nodesource_setup.sh
RUN bash nodesource_setup.sh
RUN apt install -y nodejs
RUN corepack enable yarn

RUN apt-get update && \
    apt install -y protobuf-compiler python3.11-dev clang-format

ENV POETRY_NO_INTERACTION=1 \
    POETRY_CACHE_DIR='/var/cache/pypoetry' \
    POETRY_HOME='/usr/local' \
    COREPACK_ENABLE_DOWNLOAD_PROMPT=0

RUN curl -sSL https://install.python-poetry.org | python3 -

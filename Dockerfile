# Local development image. See Makefile for usage.
FROM ubuntu:22.04

WORKDIR /app

RUN apt-get update \
    && apt-get install -y \
        cmake \
        curl \
        gcc \
        g++ \
        git \
        gnupg \
        libglib2.0-dev \
        libva-dev \
        libwebsockets-dev \
        lsb-release \
        python3-dev \
        software-properties-common \
        unzip \
    && rm -rf /var/lib/apt/lists/*

# doxygen — pin to match CI (.github/workflows/docs.yml)
ARG DOXYGEN_VERSION=1.13.2
RUN curl -fsSL https://github.com/doxygen/doxygen/releases/download/Release_$(echo ${DOXYGEN_VERSION} | tr '.' '_')/doxygen-${DOXYGEN_VERSION}.linux.bin.tar.gz \
        -o /tmp/doxygen.tar.gz \
    && tar -xzf /tmp/doxygen.tar.gz -C /tmp \
    && cp /tmp/doxygen-${DOXYGEN_VERSION}/bin/doxygen /usr/local/bin/ \
    && rm -rf /tmp/doxygen.tar.gz /tmp/doxygen-${DOXYGEN_VERSION}

# protoc — pin to match CI (arduino/setup-protoc version in .github/workflows/ci.yml)
ARG PROTOC_VERSION=29.6
RUN ARCH=$(uname -m | sed 's/aarch64/aarch_64/') \
    && curl -fsSL https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-linux-${ARCH}.zip \
        -o /tmp/protoc.zip \
    && unzip /tmp/protoc.zip -d /usr/local \
    && rm /tmp/protoc.zip

# clang
RUN curl https://apt.llvm.org/llvm.sh -fsS -o llvm.sh \
    && bash llvm.sh 19 \
    && apt-get install -y clang-tidy-19 clang-format-19 \
    && rm -rf /var/lib/apt/lists/*
ENV PATH="/usr/lib/llvm-19/bin:${PATH}"

# rust
ARG MSRV_RUST_VERSION=1.85.0
RUN curl https://sh.rustup.rs -fsS | bash -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
RUN rustup toolchain install nightly --component rust-src
RUN rustup toolchain install ${MSRV_RUST_VERSION}
RUN rustup component add rustfmt clippy

# node
RUN curl -fsSL https://deb.nodesource.com/setup_23.x -o nodesource_setup.sh \
  && bash nodesource_setup.sh \
  && apt-get update \
  && apt-get install -y nodejs \
  && rm -rf /var/lib/apt/lists/*
RUN corepack enable yarn
ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0

# python
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
ENV PATH="/root/.local/bin:${PATH}"

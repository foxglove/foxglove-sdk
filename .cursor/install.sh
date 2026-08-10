#!/usr/bin/env bash
set -euo pipefail

# The Foxglove SDK's Rust workspace targets edition 2024 (MSRV 1.88.0, see the root
# Cargo.toml). The default Cloud Agent image ships an older toolchain, so install and
# select a current stable toolchain, including the components the canonical fmt/clippy
# workflow relies on.
rustup toolchain install stable --profile minimal --no-self-update
rustup component add --toolchain stable clippy rustfmt
rustup default stable

# Warm the Cargo dependency cache so agents can build, test, and run examples without a
# cold download on first use.
cargo fetch --locked

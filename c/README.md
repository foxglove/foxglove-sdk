# C library

This crate implements a simple C interface that wraps the Rust SDK. It can be built as a static or shared library and uses [`cbindgen`](https://github.com/mozilla/cbindgen) to produce header files.

## Remote access

The `c/ra/` subdirectory contains a second crate (`foxglove_c_ra`) that compiles the same source as `c/` but with the `remote-access` feature enabled, producing a cdylib named `libfoxglove_ra`. This is distributed separately so that users who don't need LiveKit are not encumbered by its dependencies.

The two crates share `c/src/` via the `path` key in `c/ra/Cargo.toml`. Gateway-specific FFI code in `c/src/gateway.rs` is gated behind `#[cfg(feature = "remote-access")]`, so it only compiles for the RA distribution. Both distributions use the same generated header (`foxglove-c.h`); the gateway declarations are guarded by `#if defined(FOXGLOVE_REMOTE_ACCESS)`.

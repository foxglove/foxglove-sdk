# C library

This crate implements a simple C interface that wraps the Rust SDK. It can be built as a static or shared library and uses [`cbindgen`](https://github.com/mozilla/cbindgen) to produce header files.

## Remote access

Remote access support is gated behind the `remote-access` Cargo feature. When enabled, the gateway-specific FFI code in `c/src/gateway.rs` is compiled into the library. The C++ CMake build enables this via `FOXGLOVE_BUILD_REMOTE_ACCESS=ON`, which produces only a shared library (no static library). The generated header (`foxglove-c.h`) guards the gateway declarations with `#if defined(FOXGLOVE_REMOTE_ACCESS)`.

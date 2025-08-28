# C++ Foxglove Data Loader SDK

This SDK includes libraries to build a Data Loader using C++17 and the WASI SDK.

### Install the build toolchain

Download and extract the latest release from the [WASI SDK releases page](https://github.com/WebAssembly/wasi-sdk).

### Install the SDK

Download and extract the latest `foxglove_data_loader-v0.x.x-cpp-wasm32-unknown-unknown.tar.gz`
from the [Releases](https://github.com/foxglove/foxglove-sdk/releases) page. This contains:

1. The include-only library required to implement the data loader interface, in `include/foxglove_data_loader`
2. A limited subset of the [Foxglove C++ SDK](../README.md), which includes [Foxglove Schema](https://docs.foxglove.dev/docs/sdk/schemas)
   struct definitions and serialization functionality.

### Build with the SDK

Use the `clang++` binary from the extracted SDK release to compile your C++ code. In exactly one
`.cpp` file, define `FOXGLOVE_DATA_LOADER_IMPLEMENTATION` and include `foxglove_data_loader/data_loader.hpp`.

To use the Foxglove SDK to serialize messages, you will need to link the included `libfoxglove.a`
and build the C++ source files in `src/foxglove`. See the `example` target in [https://github.com/foxglove/foxglove-sdk/blob/main/cpp/foxglove_data_loader/Makefile] for an example.

Function definitions in `host_internal.h` are not intended for external use.

### Define your Data Loader implementation

You will need to define the implementation for `construct_data_loader(const DataLoaderArgs& args)`.
Use this to construct your implementation of the `foxglove_data_loader::AbstractDataLoader` interface.

See `examples/data_loader.cpp` for a simple example data loader implementation.

# C++ Foxglove Data Loader SDK

Contains an include-only framework to build a Data Loader using C++17 and the WASI SDK.

### Install the build toolchain

Download and extract the latest release from the [WASI SDK releases page](https://github.com/WebAssembly/wasi-sdk).

### Build your data loader

Use the `clang++` binary from the extracted SDK release to compile your C++ code. In exactly one
`.cpp` file, define `FOXGLOVE_DATA_LOADER_IMPLEMENTATION` and include `foxglove_data_loader/data_loader.hpp`.

You will need to define the implementation for `construct_data_loader(const DataLoaderArgs& args)`.
Use this to construct your implementation of the `foxglove_data_loader::AbstractDataLoader` interface.

See `examples/data_loader.cpp` for a simple example data loader implementation.

Function definitions in `host_internal.h` are not intended for external use.

# C++ library

The Foxglove C++ SDK is a higher-level wrapper around the [C library](../c). To build it, you will need to link with that library and add the [generated includes](../c/include) to your include paths.

The SDK headers include a copy of `expected.hpp` from [tl-expected](https://github.com/TartanLlama/expected) ([docs](https://tl.tartanllama.xyz/en/latest/api/expected.html)), which provides an implementation similar to `std::expected` from C++23.

## Local development

Build the library and examples:

```
make build
```

Run clang-format:

```
make lint
```

Run clang-tidy:

```
make CLANG_TIDY=true build
```

Build and run tests:

```
make test
```

Run with Address & Undefined Behavior sanitizers:

```
make SANITIZE=address,undefined test
```

Run example programs (note that a different `build` directory may be used depending on build settings like sanitizers):

```
./build/example_server
```

## Remote access

The remote access distribution (`libfoxglove_ra`) is a superset of the base SDK that adds the `RemoteAccessGateway` class for live visualization and teleop via the Foxglove platform.

This is distributed as a **shared library only** — no static library is provided. The underlying LiveKit/WebRTC dependency is large (hundreds of MB as a static archive) and would leak internal symbols into your binary. You must ship the shared library (`libfoxglove_ra.so`, `libfoxglove_ra.dylib`, or `foxglove_ra.dll`) alongside your application.

### Supported platforms and ABI requirements

The remote access library has strict ABI requirements inherited from the prebuilt LiveKit/WebRTC native library. **Your application must be built with a compatible compiler and runtime**, or you will encounter linker errors or undefined behavior.

| Platform | Compiler | C++ stdlib | CRT | Notes |
|----------|----------|------------|-----|-------|
| Linux x86_64 | GCC 14+ | libstdc++ | — | glibc >= 2.39 (Ubuntu 24.04+) |
| Linux aarch64 | GCC 14+ | libstdc++ | — | glibc >= 2.39 (Ubuntu 24.04+) |
| macOS x86_64 | Clang | libc++ | — | Default Xcode toolchain |
| macOS aarch64 | Clang | libc++ | — | Default Xcode toolchain |
| Windows x86_64 | MSVC | MSVC STL | `/MT` (static) | Your project must also use `/MT` |
| Windows aarch64 | MSVC | MSVC STL | `/MT` (static) | Your project must also use `/MT` |

**Not supported:** Clang/libc++ on Linux, `/MD` (dynamic CRT) on Windows.

### Building locally

```
make build-ra
```

This uses a separate build directory (`build-ra/`) from the base SDK build.

### Consuming the library

Link against the shared library and include the same C and C++ headers as the base SDK. The C++ header `foxglove/remote_access.hpp` provides the `RemoteAccessGateway` class.

The gateway-related C declarations in `foxglove-c/foxglove-c.h` are guarded by `#if defined(FOXGLOVE_REMOTE_ACCESS)`. If you are using CMake and link against the `foxglove_ra_cpp_shared` target, this define is propagated automatically. Otherwise, define `FOXGLOVE_REMOTE_ACCESS` before including the header.

## Examples

### RGB Camera Visualization Example

See detailed instructions on dependencies and visualizing data in the [example's readme](cpp/examples/rgb-camera-visualization/README.md).


#### Building the Example

Once OpenCV is installed, build the example:

```bash
make BUILD_OPENCV_EXAMPLE=ON build
```

This will create the `example_rgb_camera_visualization` executable in the build directory.

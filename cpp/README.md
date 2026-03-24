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

The remote access distribution (`libfoxglove_ra`) is a superset of the base SDK that adds the `RemoteAccessGateway` class for live visualization and teleop via the Foxglove platform. It is distributed as a **shared library only** (no static library) due to the size and symbol visibility constraints of underlying dependencies.

### Supported platforms

| Platform | Compiler | C++ stdlib | Notes |
|----------|----------|------------|-------|
| Linux x86_64 | GCC 14+ | libstdc++ | Matches livekit upstream ABI |
| Linux aarch64 | GCC 14+ | libstdc++ | |
| macOS x86_64 | Clang | libc++ | |
| macOS aarch64 | Clang | libc++ | |
| Windows x86_64 | MSVC | MSVC STL | Static CRT (`/MT`) to match livekit prebuilt webrtc |
| Windows aarch64 | MSVC | MSVC STL | Static CRT (`/MT`) |

iOS is not supported (livekit only provides `aarch64-apple-ios-sim`, which is not useful for production).

### Building locally

```
make build-ra
```

This uses a separate build directory (`build-ra/`) from the base SDK build.

### Consuming the library

Link against the shared library (`libfoxglove_ra.so`, `libfoxglove_ra.dylib`, or `foxglove_ra.dll`) and include the same C and C++ headers as the base SDK. Define `FOXGLOVE_REMOTE_ACCESS` before including `foxglove-c/foxglove-c.h` to expose the gateway C declarations. The C++ header `foxglove/remote_access.hpp` provides the `RemoteAccessGateway` class.

## Examples

### RGB Camera Visualization Example

See detailed instructions on dependencies and visualizing data in the [example's readme](cpp/examples/rgb-camera-visualization/README.md).


#### Building the Example

Once OpenCV is installed, build the example:

```bash
make BUILD_OPENCV_EXAMPLE=ON build
```

This will create the `example_rgb_camera_visualization` executable in the build directory.

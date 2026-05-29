# C++ library

The Foxglove C++ SDK is a higher-level wrapper around the [C library](../c). To build it, you will need to link with that library and add the [generated includes](../c/include) to your include paths.

The SDK headers include a copy of `expected.hpp` from [tl-expected](https://github.com/TartanLlama/expected) ([docs](https://tl.tartanllama.xyz/en/latest/api/expected.html)), which provides an implementation similar to `std::expected` from C++23.

## Dependencies

By default, CMake will try to find dependencies (Catch2, nlohmann_json, etc.) from the local package manager and fall back to building them from source via FetchContent. This is controlled by the `USE_PACKAGE_MANAGER_DEPENDENCIES` CMake option (default: `ON`).

To force all dependencies to be fetched and built from source:

```
cmake -DUSE_PACKAGE_MANAGER_DEPENDENCIES=OFF ..
```

## Local development

Build the library:

```
make build
```

To also build the example programs, set `FOXGLOVE_BUILD_EXAMPLES=ON`:

```
make build FOXGLOVE_BUILD_EXAMPLES=ON
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

Build and run integration tests:

Run livekit in repo root via `docker compose up` and then run:

```
make test-integration
```

Run with Address & Undefined Behavior sanitizers:

```
make SANITIZE=address,undefined test
```

Run example programs — these require building with `FOXGLOVE_BUILD_EXAMPLES=ON` as shown above (note that a different `build` directory may be used depending on build settings like sanitizers):

```
./build/example_server
```

## Remote access

Remote access support adds the `RemoteAccessGateway` class for live visualization and teleop via the Foxglove platform. It is built by enabling the `FOXGLOVE_REMOTE_ACCESS` CMake option, which adds the gateway code to `foxglove_cpp_shared`. Only the shared library is produced — no static library — because the LiveKit/WebRTC dependency has strict ABI requirements and would leak internal symbols into the consumer's binary.

### Supported platforms and ABI requirements

The remote access shared library has strict ABI requirements inherited from the prebuilt LiveKit/WebRTC native library. **Your application must be built with a compatible compiler and runtime**, or you will encounter linker errors or undefined behavior.

| Platform        | Compiler | C++ stdlib | CRT            | Notes                            |
| --------------- | -------- | ---------- | -------------- | -------------------------------- |
| Linux x86_64    | GCC      | libstdc++  | —              | glibc >= 2.35 (Ubuntu 22.04+)    |
| Linux aarch64   | GCC      | libstdc++  | —              | glibc >= 2.35 (Ubuntu 22.04+)    |
| macOS x86_64    | Clang    | libc++     | —              | Default Xcode toolchain          |
| macOS aarch64   | Clang    | libc++     | —              | Default Xcode toolchain          |
| Windows x86_64  | MSVC     | MSVC STL   | `/MT` (static) | Your project must also use `/MT` |
| Windows aarch64 | MSVC     | MSVC STL   | `/MT` (static) | Your project must also use `/MT` |

**Not supported:** Clang/libc++ on Linux, `/MD` (dynamic CRT) on Windows.

### Building locally

```
make build FOXGLOVE_REMOTE_ACCESS=ON
```

### Consuming the library

Link against `foxglove_cpp_shared` and include the same C and C++ headers as the base SDK. The C++ header `foxglove/remote_access.hpp` provides the `RemoteAccessGateway` class.

The gateway-related C declarations in `foxglove-c/foxglove-c.h` are guarded by `#if defined(FOXGLOVE_REMOTE_ACCESS)`. When using CMake and linking against `foxglove_cpp_shared` built with `FOXGLOVE_REMOTE_ACCESS=ON`, this define is propagated automatically. Otherwise, define `FOXGLOVE_REMOTE_ACCESS` before including the header.

## Distribution

The C++ SDK is also shipped as a redistributable dist — a self-contained directory tree with prebuilt C libraries, C++ wrapper sources, and a CMake package config consumers wire up via `find_package`. This is what downstream packages (e.g. the ROS `foxglove_bridge`) consume in lieu of a system install.

### Building the dist

From the repo root, in Docker (Ubuntu 22.04 base, for glibc 2.35 compatibility with the released artifacts):

```
make build-cpp-dist
```

Output lands at `cpp/dist/`, with layout:

```
lib/libfoxglove.a                                  # non-RA static C lib
lib/libfoxglove.so                                 # RA cdylib
lib/cmake/foxglove-sdk/foxglove-sdkConfig.cmake    # cmake glue
include/foxglove-c/                                # C headers
include/foxglove/                                  # C++ headers
src/                                               # C++ wrapper sources
```

### Consuming the dist via CMake

```cmake
find_package(foxglove-sdk CONFIG REQUIRED HINTS /path/to/dist)
foxglove_sdk_add_cpp_library(my_wrapper
    TYPE SHARED            # SHARED (default) or STATIC
    REMOTE_ACCESS ON       # ON or OFF (default)
)
target_link_libraries(my_app PRIVATE my_wrapper)
```

`foxglove_sdk_add_cpp_library` compiles the dist's C++ wrapper sources against your toolchain and links the appropriate C library.

Supported (`TYPE`, `REMOTE_ACCESS`) combinations:

| `TYPE`  | `REMOTE_ACCESS` | Result                                                                                                  |
| ------- | --------------- | ------------------------------------------------------------------------------------------------------- |
| SHARED  | ON              | Dynamic-links `libfoxglove.so` (RA), adds `remote_access.cpp`, defines `FOXGLOVE_REMOTE_ACCESS`.        |
| SHARED  | OFF             | Statically bundles `libfoxglove.a` (non-RA) into the SHARED wrapper.                                    |
| STATIC  | OFF             | Static wrapper, propagates `libfoxglove.a` to consumers.                                                |
| STATIC  | ON              | Unsupported — no static RA library is shipped (LiveKit/WebRTC has strict ABI requirements that would leak into static consumers). |

### Why the C++ wrapper is shipped as source

The C++ ABI is not stable across the toolchains downstream Linux distributions use, so the dist ships the C++ wrapper as source files that consumers compile against their own compiler. Only the C library is shipped prebuilt — its C ABI is stable.

## Examples

### RGB Camera Visualization Example

See detailed instructions on dependencies and visualizing data in the [example's readme](examples/rgb-camera-visualization/README.md).

#### Building the Example

Once OpenCV is installed, build the examples. The RGB camera example is included automatically when OpenCV is detected on your system:

```bash
make FOXGLOVE_BUILD_EXAMPLES=ON build
```

This will create the `example_rgb_camera_visualization` executable in the build directory.

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

## Examples

### RGB Camera Visualization Example

The RGB camera visualization example requires OpenCV and is not built by default.

#### Installing OpenCV Dependencies

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install libopencv-dev
```

**macOS (using Homebrew):**
```bash
brew install opencv
```

**Windows (using vcpkg):**
```bash
vcpkg install opencv
```

**From Source:**
If you prefer to build OpenCV from source, follow the [official OpenCV installation guide](https://docs.opencv.org/4.x/d7/d9f/tutorial_linux_install.html).

#### Building the Example

Once OpenCV is installed, build the example:

```bash
make BUILD_OPENCV_EXAMPLE=ON build
```

This will create the `example_rgb_camera_visualization` executable in the build directory.

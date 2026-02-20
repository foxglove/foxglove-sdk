# C++ library

The Foxglove C++ SDK is a higher-level wrapper around the [C library](../c). To build it, you will need to link with that library and add the [generated includes](../c/include) to your include paths.

The SDK headers include a copy of `expected.hpp` from [tl-expected](https://github.com/TartanLlama/expected) ([docs](https://tl.tartanllama.xyz/en/latest/api/expected.html)), which provides an implementation similar to `std::expected` from C++23.

## Message Types

Foxglove message types are available in the `foxglove::messages` namespace:

```cpp
#include <foxglove/messages.hpp>

foxglove::messages::Log log_msg;
foxglove::messages::SceneUpdate scene;
```

> **Note:** The `foxglove::schemas` namespace is deprecated. Please use `foxglove::messages` instead.
> The old namespace will continue to work as an alias for backward compatibility.

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

See detailed instructions on dependencies and visualizing data in the [example's readme](cpp/examples/rgb-camera-visualization/README.md).


#### Building the Example

Once OpenCV is installed, build the example:

```bash
make BUILD_OPENCV_EXAMPLE=ON build
```

This will create the `example_rgb_camera_visualization` executable in the build directory.

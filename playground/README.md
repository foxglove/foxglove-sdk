# SDK Playground

https://foxglove-sdk-playground.pages.dev

The SDK playground allows you to run Python code using the Foxglove SDK, and visualize the resulting data in Foxglove.

## Development

To run the playground locally, you'll need to build the Python wheel and copy it
into the `playground/public` directory. In order to build the wheel, you'll need
to configure emscripten and rust toolchains.

Pyodide's Python 3.14 runtime uses the [`pyemscripten_2026_0`
platform](https://pyodide.org/en/stable/development/abi/314.html), which
requires Emscripten 5.0.3 and rust 1.93.0 or later.

Set up the emscripten toolchain with
[emsdk](https://github.com/emscripten-core/emsdk).

```sh
git clone https://github.com/emscripten-core/emsdk.git
emsdk/emsdk install 5.0.3
emsdk/emsdk activate 5.0.3
source emsdk/emsdk_env.sh
```

Add the `wasm32-unknown-emscripten` target to your rust toolchain:

```sh
rustup target add wasm32-unknown-emscripten
```

Now you can build the wheel.

```sh
cd ../python/foxglove-sdk
CFLAGS=-fPIC \
  MATURIN_PYEMSCRIPTEN_PLATFORM_VERSION=2026_0 \
  uv run maturin build --release --out dist --target wasm32-unknown-emscripten -i python3.12
cp dist/foxglove_sdk-*.whl ../../playground/public/
```

Then run the dev server:

```sh
cd ../../playground
yarn start
```

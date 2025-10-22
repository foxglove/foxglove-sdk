# SDK Playground

https://foxglove-sdk-playground.pages.dev

The SDK playground allows you to run Python code using the Foxglove SDK, and visualize the resulting data in Foxglove.

## Development

To run the playground locally, you'll need to build the Python wheel and copy it
into the `playground/public` directory. In order to build the wheel, you'll need
to configure emscripten and rust toolchains.

Set up the emscripten toolchain with
[emsdk](https://github.com/emscripten-core/emsdk).

```sh
git clone https://github.com/emscripten-core/emsdk.git
emsdk/emsdk install 3.1.58
emsdk/emsdk activate 3.1.58
source emsdk/emsdk_env.sh
```

We currently use an older version of rust to work around limitations in this
version of emscripten. To install the 1.86.0 toolchain:

```sh
rustup toolchain install 1.86.0
rustup target add wasm32-unknown-emscripten --toolchain 1.86.0
```

Now you can build the wheel.

```sh
cd ../python/foxglove-sdk
CFLAGS=-fPIC \
  RUSTC_BOOTSTRAP=1 \
  RUSTUP_TOOLCHAIN=1.86.0 \
  uv run maturin build --release --out dist --target wasm32-unknown-emscripten -i python3.12
cp dist/foxglove_sdk-*.whl ../../playground/public
```

Then run the dev server:

```sh
cd ../../playground
yarn start
```

# SDK Playground

https://foxglove-sdk-playground.pages.dev

The SDK playground allows you to run Python code using the Foxglove SDK, and visualize the resulting data in Foxglove.

## Development

To run the playground locally, first build the Python wheel and copy it into the `playground/public` directory:

```sh
cd ../python/foxglove-sdk
CFLAGS=-fPIC RUSTC_BOOTSTRAP=1 poetry run maturin build --release --out dist --target wasm32-unknown-emscripten -i python3.12
cp dist/foxglove_sdk-*.whl ../playground/public
```

Then run the dev server:

```sh
cd ../playground
yarn start
```

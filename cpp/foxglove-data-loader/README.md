# wit c++ example extension

# install and build

install [wasi-sdk](https://github.com/WebAssembly/wasi-sdk.git)
and set `$WASI_SDK` to the build/install directory:

```
export WASI_SDK=~/dev/wasi-sdk/build/install
```

install rust and do:

```
rustup target add wasm32-unknown-unknown
```

then build the extension:

```
npm install
npm run build
npm run package
```

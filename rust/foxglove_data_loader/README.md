# Foxglove Data Loader

Data Loaders are an experimental [Extension API] that allow you to support more file formats in
Foxglove. You build a Data Loader as a Foxglove Extension using WASM.

[Extension API]: https://docs.foxglove.dev/docs/extensions

# Example

In your data loader code, implement `DataLoader` and `MessageIterator` traits with your file format
parser and iteration code:

``` rs
use foxglove_data_loader::{
    reader, console, BackfillArgs, Channel, DataLoader, DataLoaderArgs, Initialization,
    Message, MessageIterator, MessageIteratorArgs, Schema, TimeRange,
};
foxglove_data_loader::export!(MyLoader);

struct MyLoader { /* ... */ }

impl DataLoader for MyLoader {
    type MessageIterator = MyIterator;
    type Error = anyhow::Error;

    fn new(args: DataLoaderArgs) -> Self {
        // args.paths is a Vec<String> of file paths you can reader::open()
        unimplemented![]
    }

    fn initialize(&mut self) -> Result<Initialization, Self::Error> {
        // the Initialization contains the channels, schemas, time range, and any problems
        unimplemented![]
    }

    fn create_iter(&mut self, args: MessageIteratorArgs) -> Result<Self::MessageIterator, Self::Error> {
        unimplemented![]
    }

    fn get_backfill(&mut self, args: BackfillArgs) -> Result<Vec<Message>, Self::Error> {
        unimplemented![]
    }
}

struct MyIterator { /* ... */ }

impl MessageIterator for MyIterator {
    type Error = anyhow::Error;

    fn next(&mut self) -> Option<Result<Message, Self::Error>> {
        unimplemented![]
    }
}
```

To read data from files in your loader, use `reader::open(&path)`. The return value implements
`std::io::Read` and `std::io::Seek`, so you can use higher-level adaptors such as `BufReader`:

``` rs
let reader = foxglove_data_loader::reader::open(&path);
let mut lines = BufReader::new(reader).lines();
```

Then you can build for `wasm32-unknown-unknown` to get a `.wasm` file:

```
$ cargo build --release --target wasm32-unknown-unknown

$ find -name \*.wasm
./target/wasm32-unknown-unknown/release/my_foxglove_data_loader.wasm
```

In your [foxglove extension], pass this WASM file as `wasmUrl` (inline `data:` or otherwise) to
`extensionContext.registerDataLoader()`:

``` ts
import { Experimental } from "@foxglove/extension";
import wasmUrl from "./target/wasm32-unknown-unknown/release/my_foxglove_data_loader.wasm";

export function activate(extensionContext: Experimental.ExtensionContext): void {
  extensionContext.registerDataLoader({
    type: "file",
    wasmUrl,
    supportedFileType: ".xyz",
  });
}
```

For a more complete example, check out the [rust-data-loader][] example in the
[create-foxglove-extension][] repo.

[rust-data-loader]: https://github.com/foxglove/create-foxglove-extension/tree/main/examples
[create-foxglove-extension]: https://github.com/foxglove/create-foxglove-extension
[foxglove extension]: https://docs.foxglove.dev/docs/visualization/extensions/introduction

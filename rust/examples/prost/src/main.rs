use foxglove::{ChannelBuilder, McapWriter, Schema};
use prost::Message;

pub mod fruit {
    include!("../generated/fruit.rs");
}

const APPLE_SCHEMA: &[u8] = include_bytes!("../generated/apple.fdset");

/// This example shows how to log custom protobuf messages to an MCAP file, using the
/// [prost](https://docs.rs/prost) crate.
///
/// To run this example, in addition to the `prost` and `prost-build` crates, you must install a
/// [protobuf compiler](https://github.com/protocolbuffers/protobuf#protobuf-compiler-installation).
fn main() {
    let writer = McapWriter::new()
        .create_new_buffered_file("fruit.mcap")
        .expect("failed to create writer");

    // Set up a channel for our protobuf messages
    let schema = Schema::new("fruit.Apple", "protobuf", APPLE_SCHEMA);
    let channel = ChannelBuilder::new("/fruit")
        .message_encoding("protobuf")
        .schema(schema)
        .build_raw()
        .expect("failed to build channel");

    // Create and log a protobuf message
    let msg = fruit::Apple {
        color: Some("red".to_string()),
        diameter: Some(10),
    };
    let mut buf = vec![];
    msg.encode(&mut buf).expect("failed to encode");

    channel.log(&buf);

    writer.close().expect("failed to close writer");
}

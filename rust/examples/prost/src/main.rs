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
    let channel = ChannelBuilder::new("/fruit").build::<fruit::Apple>();

    // Create and log a protobuf message
    let msg = fruit::Apple {
        color: Some("red".to_string()),
        diameter: Some(10),
    };
    channel.log(&msg);

    writer.close().expect("failed to close writer");
}

/// An implementation of the `Encode` trait for the `Apple` message, which encapsulates information
/// needed to construct a `Channel`. This isn't required if you want to use a `RawChannel`.
impl foxglove::Encode for fruit::Apple {
    type Error = prost::EncodeError;

    fn get_schema() -> Option<Schema> {
        Some(Schema::new("fruit.Apple", "protobuf", APPLE_SCHEMA))
    }

    fn get_message_encoding() -> String {
        "protobuf".to_string()
    }

    fn encode(&self, buf: &mut impl prost::bytes::BufMut) -> Result<(), Self::Error> {
        Message::encode(self, buf)?;
        Ok(())
    }
}

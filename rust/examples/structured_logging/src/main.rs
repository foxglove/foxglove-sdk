use bytes::Bytes;
use chrono::{DateTime, Utc};
use foxglove::{Encode, LazyChannel, LazyRawChannel, McapWriteOptions, McapWriter, schemas};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct Apple {
    color: String,
    diameter: f64,
}

#[derive(Encode, Debug)]
struct Banana {
    length: f64,
    ripeness: f64,
    picked_at: Option<DateTime<Utc>>,
}

// This channel logs images using Foxglove's image schema
static IMG_CHANNEL: LazyChannel<schemas::CompressedImage> = LazyChannel::new("/image");
// This channel logs schemaless JSON
static SCHEMALESS_CHANNEL: LazyRawChannel = LazyRawChannel::new("/schemaless", "json");
// This channel logs JSON with a jsonschema
static POINTS_CHANNEL: LazyRawChannel = LazyRawChannel::new("/points", "json").schema(
    "point",
    "jsonschema",
    r#"{"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"}}}"#
        .as_bytes(),
);
// This channel logs a custom type as JSON using the schemars feature to generate the jsonschema automatically
static APPLE_CHANNEL: LazyChannel<Apple> = LazyChannel::new("/apple");
// This channel logs a custom type as protobuf, using foxglove_derive
// Note: this doesn't give you control over field tags,
// it's fine as a quick-and-dirty way to serialize a struct
// but limited for evolving a type over time.
static BANANA_CHANNEL: LazyChannel<Banana> = LazyChannel::new("/banana");
// It's also possible to use a protobuf library and generate the types
// Or to use any other serialization format, see the the prost-protobuf example.

const IMG_DATA: &[u8] = include_bytes!("fox.webp");

fn main() {
    let env = env_logger::Env::default().default_filter_or("debug");
    env_logger::init_from_env(env);

    let options = McapWriteOptions::new();
    let writer = McapWriter::with_options(options)
        .create_new_buffered_file("example.mcap")
        .expect("Failed to start mcap writer");

    // Using the Foxglove CompressedImage schema
    IMG_CHANNEL.log(&schemas::CompressedImage {
        data: Bytes::from_static(IMG_DATA),
        format: "webp".to_string(),
        ..Default::default()
    });

    // For a raw channel, it expects bytes
    SCHEMALESS_CHANNEL.log(
        serde_json::json!({
            "prompt": "What's the answer to life, the universe, and everything?",
            "answer": 42,
        })
        .to_string()
        .as_bytes(),
    );

    // The schema is for consumers of the data, no validation is performed
    POINTS_CHANNEL.log(
        serde_json::json!({
            "x": 1,
            "y": 2,
        })
        .to_string()
        .as_bytes(),
    );

    // For a typed JSON channel, we can just pass the struct directly.
    // The type system ensures conformance with the schema.
    APPLE_CHANNEL.log(&Apple {
        color: "red".to_string(),
        diameter: 10.0,
    });

    // Or if we just want to serialize a struct without caring about the encoding,
    // we can use the foxglove_derive feature to serialize to binary protobuf automatically.
    BANANA_CHANNEL.log(&Banana {
        length: 10.0,
        ripeness: 0.5,
        picked_at: Some(Utc::now()),
    });

    writer.close().expect("Failed to flush mcap file");
}

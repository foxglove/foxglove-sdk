pub mod client;
pub mod server;

/// Returns true if the given `schema_encoding` is one of the types that is known to require binary
/// schema data (i.e. `protobuf` and `flatbuffer`). These require base64-encoding/decoding to be
/// sent via JSON messages on the websocket connection.
fn is_known_binary_schema_encoding<T: AsRef<str>>(schema_encoding: T) -> bool {
    schema_encoding.as_ref() == "protobuf" || schema_encoding.as_ref() == "flatbuffer"
}

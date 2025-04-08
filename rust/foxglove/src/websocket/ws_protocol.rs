//! Implementation of the Foxglove WebSocket protocol

pub mod client;
mod message;
pub mod parameter;
mod parse_error;
pub mod schema;
pub mod server;
pub mod tungstenite;

pub use message::{BinaryMessage, JsonMessage};
pub use parse_error::ParseError;

//! Common functionality shared across Foxglove protocol versions

pub mod client;
mod message;
pub mod parameter;
mod parse_error;
pub mod schema;
pub mod server;

pub use message::{BinaryMessage, BinaryPayload, JsonMessage};
pub use parameter::DecodeError;
pub use parse_error::ParseError;

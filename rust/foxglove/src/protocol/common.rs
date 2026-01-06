//! Common functionality shared across Foxglove protocol versions

pub mod client;
mod message;
pub mod parameter;
mod parse_error;
pub mod schema;
pub mod server;
pub mod tungstenite;

pub use message::{BinaryMessage, JsonMessage};
pub use parse_error::ParseError;

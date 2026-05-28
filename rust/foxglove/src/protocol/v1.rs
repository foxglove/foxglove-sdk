//! Foxglove protocol v1 types.

pub mod client;
mod message;
pub mod server;
#[cfg(feature = "websocket")]
pub mod tungstenite;

pub use crate::protocol::common::{JsonMessage, ParseError};
pub use crate::protocol::common::{parameter, schema};
pub use message::BinaryMessage;

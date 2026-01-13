//! Foxglove protocol v1 types.

pub mod client;
pub mod server;
pub mod tungstenite;

pub use crate::protocol::common::{parameter, schema};
pub use crate::protocol::common::{BinaryMessage, JsonMessage, ParseError};

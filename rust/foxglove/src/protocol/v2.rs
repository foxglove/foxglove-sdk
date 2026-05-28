//! Foxglove protocol v2 types.

pub mod client;
mod message;
pub mod server;

pub use crate::protocol::common::DecodeError;
pub use crate::protocol::common::JsonMessage;
pub use crate::protocol::common::parameter;
#[allow(unused_imports)]
pub use crate::protocol::v2::message::BinaryMessage;

//! Foxglove protocol v2 types.

pub mod client;
mod message;
pub mod server;

#[allow(unused_imports)]
pub use crate::protocol::common::JsonMessage;

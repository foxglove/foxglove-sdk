//! Implementation of the Foxglove WebSocket protocol
//!
//! This crate provides types and functions for implementing the Foxglove WebSocket protocol.
//! For more information about the protocol, see the [specification](https://github.com/foxglove/ws-protocol).
//!
//! # Features
//!
//! - `tungstenite` - Enables integration with the `tungstenite` WebSocket library

#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod client;
mod message;
pub mod parameter;
mod parse_error;
pub mod schema;
pub mod server;
#[cfg(feature = "tungstenite")]
pub mod tungstenite;

pub use message::{BinaryMessage, JsonMessage};
pub use parse_error::ParseError;

#![doc(hidden)]
#![cfg_attr(
    docsrs,
    doc(cfg(any(feature = "remote-access", feature = "websocket")))
)]
//! Implementation of the Foxglove protocol

// Common is kept internal to the protocol module; common messages are re-exported from specific protocol version modules as-needed.
// End users should only use a specific protocol version module.
#[cfg_attr(
    any(not(feature = "websocket"), not(feature = "remote-access")),
    allow(unused)
)]
mod common;
use common::{BinaryMessage, BinaryPayload, JsonMessage, ParseError};
use common::{parameter, schema};

// Protocol v1, used by the Foxglove WebSocket server/client
#[cfg(feature = "websocket")]
pub mod v1;

// Protocol v2
#[allow(unused)]
#[cfg(feature = "remote-access")]
pub mod v2;

//! Implementation of the Foxglove protocol

// Common is kept internal to the protocol module; common messages are re-exported from specific protocol version modules as-needed.
// End users should only use a specific protocol version module.
#[doc(hidden)]
mod common;
use common::{parameter, schema};
use common::{BinaryPayload, JsonMessage, ParseError};

// Protocol v1, used by the Foxglove WebSocket server/client
#[doc(hidden)]
pub mod v1;

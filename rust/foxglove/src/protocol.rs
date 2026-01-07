//! Implementation of the Foxglove protocol

#[doc(hidden)]
pub mod common;
#[doc(hidden)]
pub mod v1;

pub use common::{parameter, schema};
pub use common::{BinaryMessage, JsonMessage, ParseError};

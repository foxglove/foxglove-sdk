//! WebSocket-specific aliases for the shared parameter handler types.
//!
//! See [`crate::remote_common::parameters`] for the underlying generic trait and responders;
//! this module just instantiates them with the WebSocket [`Client`] type.

use super::Client;
use crate::remote_common::parameters;

pub use parameters::{ParameterHandler, SendParameterResponse};

/// Type alias for the WebSocket-specific Get responder.
pub type GetParametersResponder = parameters::GetParametersResponder<Client>;

/// Type alias for the WebSocket-specific Set responder.
pub type SetParametersResponder = parameters::SetParametersResponder<Client>;

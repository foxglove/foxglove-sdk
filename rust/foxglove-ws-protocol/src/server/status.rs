//! Status message types.

use crate::JsonMessage;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

/// Level indicator for a [`Status`] message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
#[allow(missing_docs)]
pub enum Level {
    Info = 0,
    Warning = 1,
    Error = 2,
}

/// Status message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#status>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "status", rename_all = "camelCase")]
pub struct Status {
    /// Log level.
    pub level: Level,
    /// Message.
    pub message: String,
    /// Optional identifier for the status message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl Status {
    /// Creates a new status message.
    pub fn new(level: Level, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            id: None,
        }
    }

    /// Creates a new info-level message.
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(Level::Info, message)
    }

    /// Creates a new warning-level message.
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(Level::Warning, message)
    }

    /// Creates a new error-level message.
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Level::Error, message)
    }

    /// Sets the status message ID, so that this status can be replaced or removed in the future.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl JsonMessage for Status {}

#[cfg(test)]
mod tests {
    use crate::server::ServerMessage;

    use super::*;

    fn message() -> Status {
        Status::warning("Oh no").with_id("status-123")
    }

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(message());
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = orig.to_string();
        let msg = ServerMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ServerMessage::Status(orig));
    }
}

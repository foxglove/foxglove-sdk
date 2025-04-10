use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

/// Service call failure message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#service-call-failure>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "serviceCallFailure", rename_all = "camelCase")]
pub struct ServiceCallFailure {
    /// Service ID.
    pub service_id: u32,
    /// Call ID.
    pub call_id: u32,
    /// Error message.
    pub message: String,
}

impl ServiceCallFailure {
    /// Creates a new service call failure message.
    pub fn new(service_id: u32, call_id: u32, message: impl Into<String>) -> Self {
        Self {
            service_id,
            call_id,
            message: message.into(),
        }
    }
}

impl JsonMessage for ServiceCallFailure {}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::server::ServerMessage;

    use super::*;

    fn message() -> ServiceCallFailure {
        ServiceCallFailure {
            service_id: 1,
            call_id: 1,
            message: "Service does not exist".into(),
        }
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
        assert_eq!(msg, ServerMessage::ServiceCallFailure(orig));
    }
}

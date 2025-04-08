use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

/// Unadvertise services message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#unadvertise-services>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "unadvertiseServices", rename_all = "camelCase")]
pub struct UnadvertiseServices {
    /// IDs of the services to unadvertise.
    pub service_ids: Vec<u32>,
}

impl UnadvertiseServices {
    /// Creates a new unadvertise services message.
    pub fn new(service_ids: impl IntoIterator<Item = u32>) -> Self {
        Self {
            service_ids: service_ids.into_iter().collect(),
        }
    }
}

impl JsonMessage for UnadvertiseServices {}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::server::ServerMessage;

    use super::*;

    fn message() -> UnadvertiseServices {
        UnadvertiseServices {
            service_ids: vec![1, 2],
        }
    }

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(message());
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = serde_json::to_string(&orig).unwrap();
        let msg = ServerMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ServerMessage::UnadvertiseServices(orig));
    }
}

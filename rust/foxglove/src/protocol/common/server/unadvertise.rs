use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

/// Unadvertise message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#unadvertise>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "unadvertise", rename_all = "camelCase")]
pub struct Unadvertise {
    /// IDs of the channels to unadvertise.
    pub channel_ids: Vec<u64>,
}

impl Unadvertise {
    /// Creates a new unadvertise message with the given channel IDs.
    pub fn new(channel_ids: impl IntoIterator<Item = u64>) -> Self {
        Self {
            channel_ids: channel_ids.into_iter().collect(),
        }
    }
}

impl JsonMessage for Unadvertise {}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::server::ServerMessage;

    use super::*;

    fn message() -> Unadvertise {
        Unadvertise::new([1, 2, 3])
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
        assert_eq!(msg, ServerMessage::Unadvertise(orig));
    }
}

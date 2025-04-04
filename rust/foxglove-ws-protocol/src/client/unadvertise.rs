use crate::JsonMessage;
use serde::{Deserialize, Serialize};

/// Client unadvertise message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#client-unadvertise>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "unadvertise", rename_all = "camelCase")]
pub struct Unadvertise {
    /// Channel IDs.
    pub channel_ids: Vec<u32>,
}

impl JsonMessage for Unadvertise {}

#[cfg(test)]
mod tests {
    use crate::client::ClientMessage;

    use super::*;

    fn message() -> Unadvertise {
        Unadvertise {
            channel_ids: vec![1, 2, 3],
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
        let msg = ClientMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ClientMessage::Unadvertise(orig));
    }
}

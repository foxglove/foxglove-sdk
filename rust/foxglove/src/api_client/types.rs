use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeviceResponse {
    pub id: String,
    pub name: String,
    pub project_id: String,
    pub retain_recordings_seconds: Option<u64>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ErrorResponse {
    #[serde(rename = "error")]
    pub message: String,
    pub code: Option<String>,
}

/// Query parameters for the watch endpoint.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatchQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_access_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_watch_lease_id: Option<String>,
}

/// Payload of the `hello` event emitted when the watch stream is first established.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatchHelloEvent {
    pub watch_lease_id: String,
    pub device_wait_for_viewer_ms: u64,
    pub heartbeat_interval_ms: u64,
}

/// Payload of a `wake` event emitted when viewers are waiting for the device.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatchWakeEvent {
    #[serde(default)]
    pub remote_access_session_id: Option<String>,
    pub url: String,
    pub token: String,
}

/// Body of a heartbeat POST.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WatchHeartbeatRequest<'a> {
    pub watch_lease_id: &'a str,
}

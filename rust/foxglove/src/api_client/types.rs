#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_access_session_id: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RtcCredentials {
    /// Expiring access token (JWT)
    pub token: String,
    /// URL of the RTC server where these credentials are valid.
    pub url: String,
    /// Session ID for log correlation across components. Either echoed from the request
    /// or server-generated when the client did not provide one.
    pub remote_access_session_id: Option<String>,
}

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

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RtcCredentials {
    pub token: String,
    pub url: String,
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

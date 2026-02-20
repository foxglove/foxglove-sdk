use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::{Json, Router};
use serde::Serialize;
use tokio::net::TcpListener;

use crate::livekit_token;

pub const TEST_DEVICE_TOKEN: &str = "fox_dt_testtoken";
pub const TEST_DEVICE_ID: &str = "dev_testdevice";
const TEST_DEVICE_NAME: &str = "test-device";
const TEST_PROJECT_ID: &str = "prj_testproj";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceResponse {
    id: String,
    name: String,
    project_id: String,
    retain_recordings_seconds: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RtcCredentials {
    token: String,
    url: String,
}

struct MockState {
    room_name: String,
}

pub struct MockServerHandle {
    url: String,
    join_handle: tokio::task::JoinHandle<()>,
}

impl MockServerHandle {
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for MockServerHandle {
    fn drop(&mut self) {
        self.join_handle.abort();
    }
}

/// Starts a mock Foxglove API server that returns LiveKit tokens for the local dev server.
pub async fn start_mock_server(room_name: &str) -> MockServerHandle {
    let state = Arc::new(MockState {
        room_name: room_name.to_string(),
    });

    let app = Router::new()
        .route(
            "/internal/platform/v1/device-info",
            axum::routing::get(device_info_handler),
        )
        .route(
            "/internal/platform/v1/devices/{device_id}/remote-sessions",
            axum::routing::post(authorize_handler),
        )
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let join_handle = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    MockServerHandle {
        url: format!("http://{addr}"),
        join_handle,
    }
}

fn validate_device_token(headers: &HeaderMap) -> Result<(), StatusCode> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if auth != format!("DeviceToken {TEST_DEVICE_TOKEN}") {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}

async fn device_info_handler(headers: HeaderMap) -> Result<Json<DeviceResponse>, StatusCode> {
    validate_device_token(&headers)?;
    Ok(Json(DeviceResponse {
        id: TEST_DEVICE_ID.into(),
        name: TEST_DEVICE_NAME.into(),
        project_id: TEST_PROJECT_ID.into(),
        retain_recordings_seconds: Some(3600),
    }))
}

async fn authorize_handler(
    Path(device_id): Path<String>,
    State(state): State<Arc<MockState>>,
    headers: HeaderMap,
) -> Result<Json<RtcCredentials>, StatusCode> {
    validate_device_token(&headers)?;
    if device_id != TEST_DEVICE_ID {
        return Err(StatusCode::NOT_FOUND);
    }

    // Generate a real LiveKit JWT for the local dev server, with the device as identity.
    let token = livekit_token::generate_token(&state.room_name, TEST_DEVICE_ID)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(RtcCredentials {
        token,
        url: livekit_token::LIVEKIT_URL.to_string(),
    }))
}

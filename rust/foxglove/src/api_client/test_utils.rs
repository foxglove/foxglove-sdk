use axum::http::HeaderMap;
use axum::{Json, Router};
use reqwest::StatusCode;
use tokio::net::TcpListener;

use super::client::{DeviceToken, FoxgloveApiClient, FoxgloveApiClientBuilder};
use super::types::DeviceResponse;

pub const TEST_DEVICE_TOKEN: &str = "fox_dt_testtoken";
pub const TEST_DEVICE_ID: &str = "dev_testdevice";
pub const TEST_PROJECT_ID: &str = "prj_testproj";

pub struct ServerHandle {
    url: String,
    join_handle: tokio::task::JoinHandle<()>,
}

impl ServerHandle {
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.join_handle.abort()
    }
}

/// Starts a test server exposing the device-info endpoint.
pub async fn create_test_server() -> ServerHandle {
    let app = Router::new().route(
        "/internal/platform/v1/device-info",
        axum::routing::any(device_info_handler),
    );

    let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let join_handle = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    ServerHandle {
        url: format!("http://{addr}"),
        join_handle,
    }
}

/// Creates a test API client pointed at the given base URL.
pub fn create_test_api_client(
    url: &str,
    device_token: DeviceToken,
) -> FoxgloveApiClient<DeviceToken> {
    FoxgloveApiClientBuilder::new(device_token)
        .base_url(url)
        .build()
        .unwrap()
}

async fn device_info_handler(headers: HeaderMap) -> Result<Json<DeviceResponse>, StatusCode> {
    let auth = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if auth != format!("DeviceToken {TEST_DEVICE_TOKEN}") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(Json(DeviceResponse {
        id: TEST_DEVICE_ID.into(),
        name: "Test Device".into(),
        project_id: TEST_PROJECT_ID.into(),
        retain_recordings_seconds: Some(3600),
    }))
}

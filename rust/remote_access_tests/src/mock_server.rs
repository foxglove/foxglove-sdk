use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{Json, Router};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use crate::livekit_token;

pub const TEST_DEVICE_TOKEN: &str = "fox_dt_testtoken";
pub const TEST_DEVICE_ID: &str = "dev_testdevice";
const TEST_DEVICE_NAME: &str = "test-device";
const TEST_PROJECT_ID: &str = "prj_testproj";

/// Heartbeat cadence advertised by the mock to a connecting gateway. Short enough that lost
/// heartbeats are detected quickly during tests, but long enough that the SSE read-timeout
/// (a multiple of this) does not race normal test setup.
const MOCK_HEARTBEAT_INTERVAL_MS: u64 = 5_000;
/// Idle timeout advertised by the mock. Picked large so existing tests, which leave viewers
/// connected for a few seconds at most, do not trigger a session-end transition.
const MOCK_DEVICE_WAIT_FOR_VIEWER_MS: u64 = 300_000;

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
struct WatchHello {
    watch_lease_id: String,
    device_wait_for_viewer_ms: u64,
    heartbeat_interval_ms: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WatchWake {
    remote_access_session_id: String,
    url: String,
    token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeartbeatBody {
    #[serde(default)]
    #[allow(dead_code)]
    watch_lease_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WatchQueryParams {
    #[serde(default)]
    #[allow(dead_code)]
    protocol_version: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    remote_access_session_id: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    previous_watch_lease_id: Option<String>,
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

/// Starts a mock Foxglove API server that emits a watch stream wake carrying a LiveKit
/// access token for the local dev server, and accepts heartbeats.
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
            "/internal/platform/v1/remote-sessions/watch",
            axum::routing::get(watch_handler),
        )
        .route(
            "/internal/platform/v1/remote-sessions/watch/heartbeat",
            axum::routing::post(heartbeat_handler),
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

/// Emits a `hello` event followed immediately by a `wake` event carrying a freshly-minted
/// LiveKit token for the configured room. After the wake the gateway closes its end of the
/// stream, so we don't need to keep it open.
async fn watch_handler(
    Query(_query): Query<WatchQueryParams>,
    State(state): State<Arc<MockState>>,
    headers: HeaderMap,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, axum::Error>>>, StatusCode> {
    validate_device_token(&headers)?;
    let token = livekit_token::generate_token(&state.room_name, TEST_DEVICE_ID)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let lease_id = format!(
        "rwl_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let hello = WatchHello {
        watch_lease_id: lease_id,
        device_wait_for_viewer_ms: MOCK_DEVICE_WAIT_FOR_VIEWER_MS,
        heartbeat_interval_ms: MOCK_HEARTBEAT_INTERVAL_MS,
    };
    let wake = WatchWake {
        remote_access_session_id: "ras_0000mockSession".into(),
        url: livekit_token::livekit_url(),
        token,
    };

    let events = vec![
        Event::default()
            .event("hello")
            .json_data(&hello)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        Event::default()
            .event("wake")
            .json_data(&wake)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    ];
    let body_stream = stream::iter(events.into_iter().map(Ok));
    Ok(Sse::new(body_stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

async fn heartbeat_handler(
    headers: HeaderMap,
    Json(_body): Json<HeartbeatBody>,
) -> Result<StatusCode, StatusCode> {
    validate_device_token(&headers)?;
    Ok(StatusCode::OK)
}

#![allow(dead_code)]

use std::fmt::Display;
use std::time::Duration;

use percent_encoding::AsciiSet;
use reqwest::header::{HeaderMap, AUTHORIZATION, USER_AGENT};
use reqwest::{Method, StatusCode};
use thiserror::Error;

use super::types::{DeviceResponse, ErrorResponse, RtcCredentials};

pub(super) const DEFAULT_API_URL: &str = "https://api.foxglove.dev";

const PATH_ENCODING: AsciiSet = percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

pub(super) fn encode_uri_component(component: &str) -> impl Display + '_ {
    percent_encoding::percent_encode(component.as_bytes(), &PATH_ENCODING)
}

#[derive(Clone)]
pub(crate) struct DeviceToken(String);

impl DeviceToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    fn to_header(&self) -> String {
        format!("DeviceToken {}", self.0)
    }
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub(crate) enum RequestError {
    #[error("failed to send request: {0}")]
    SendRequest(#[source] reqwest::Error),

    #[error("failed to load response bytes: {0}")]
    LoadResponseBytes(#[source] reqwest::Error),

    #[error("received error response {status}: {error:?}")]
    ErrorResponse {
        status: StatusCode,
        error: ErrorResponse,
        headers: Box<HeaderMap>,
    },

    #[error("received malformed error response {status} with body '{body}'")]
    MalformedErrorResponse {
        status: StatusCode,
        body: String,
        headers: Box<HeaderMap>,
    },

    #[error("failed to parse response: {0}")]
    ParseResponse(#[source] serde_json::Error),
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub(crate) enum FoxgloveApiClientError {
    #[error(transparent)]
    Request(#[from] RequestError),

    #[error("failed to build client: {0}")]
    BuildClient(#[from] reqwest::Error),

    #[error("no token provided")]
    NoToken(),
}

impl FoxgloveApiClientError {
    pub fn status_code(&self) -> Option<StatusCode> {
        match self {
            Self::Request(
                RequestError::MalformedErrorResponse { status, .. }
                | RequestError::ErrorResponse { status, .. },
            ) => Some(*status),
            _ => None,
        }
    }
}

#[must_use]
pub(super) struct RequestBuilder(reqwest::RequestBuilder);

impl RequestBuilder {
    fn new(client: &reqwest::Client, method: Method, url: &str, user_agent: &str) -> Self {
        Self(client.request(method, url).header(USER_AGENT, user_agent))
    }

    pub fn device_token(mut self, token: &DeviceToken) -> Self {
        self.0 = self.0.header(AUTHORIZATION, token.to_header());
        self
    }

    pub async fn send(self) -> Result<reqwest::Response, RequestError> {
        let response = self.0.send().await.map_err(RequestError::SendRequest)?;

        let status = response.status();
        if status.is_client_error() || status.is_server_error() {
            let headers = Box::new(response.headers().clone());
            let body = response.bytes().await.unwrap_or_default();
            match serde_json::from_slice::<ErrorResponse>(&body) {
                Ok(error) => {
                    return Err(RequestError::ErrorResponse {
                        status,
                        error,
                        headers,
                    });
                }
                Err(_) => {
                    let body = String::from_utf8_lossy(&body).to_string();
                    return Err(RequestError::MalformedErrorResponse {
                        status,
                        body,
                        headers,
                    });
                }
            }
        }

        Ok(response)
    }
}

pub(super) fn default_user_agent() -> String {
    format!("foxglove-sdk/{}", env!("CARGO_PKG_VERSION"))
}

#[derive(Clone)]
pub(super) struct FoxgloveApiClient {
    http: reqwest::Client,
    device_token: Option<DeviceToken>,
    base_url: String,
    user_agent: String,
}

impl FoxgloveApiClient {
    pub fn new(
        base_url: impl Into<String>,
        device_token: Option<DeviceToken>,
        user_agent: impl Into<String>,
    ) -> Result<Self, FoxgloveApiClientError> {
        Ok(Self {
            http: reqwest::ClientBuilder::new().build()?,
            device_token,
            base_url: base_url.into(),
            user_agent: user_agent.into(),
        })
    }

    pub fn set_device_token(&mut self, token: DeviceToken) -> &mut Self {
        self.device_token = Some(token);
        self
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        );
        RequestBuilder::new(&self.http, method, &url, &self.user_agent)
    }

    pub fn get(&self, endpoint: &str) -> RequestBuilder {
        self.request(Method::GET, endpoint)
    }

    pub fn post(&self, endpoint: &str) -> RequestBuilder {
        self.request(Method::POST, endpoint)
    }

    pub fn device_token(&self) -> Option<&DeviceToken> {
        self.device_token.as_ref()
    }

    pub async fn fetch_device_info(&self) -> Result<DeviceResponse, FoxgloveApiClientError> {
        let Some(token) = self.device_token() else {
            return Err(FoxgloveApiClientError::NoToken());
        };

        let response = self
            .get("/internal/platform/v1/device-info")
            .device_token(token)
            .send()
            .await?;

        let bytes = response
            .bytes()
            .await
            .map_err(super::client::RequestError::LoadResponseBytes)?;

        serde_json::from_slice(&bytes).map_err(|e| {
            FoxgloveApiClientError::Request(super::client::RequestError::ParseResponse(e))
        })
    }

    pub async fn authorize_remote_viz(
        &self,
        device_id: &str,
    ) -> Result<RtcCredentials, FoxgloveApiClientError> {
        let Some(device_token) = self.device_token() else {
            return Err(FoxgloveApiClientError::NoToken());
        };

        let device_id = encode_uri_component(device_id);
        let response = self
            .post(&format!(
                "/internal/platform/v1/devices/{device_id}/remote-sessions"
            ))
            .device_token(device_token)
            .send()
            .await?;

        let bytes = response
            .bytes()
            .await
            .map_err(super::client::RequestError::LoadResponseBytes)?;

        serde_json::from_slice(&bytes).map_err(|e| {
            FoxgloveApiClientError::Request(super::client::RequestError::ParseResponse(e))
        })
    }
}

pub(super) struct FoxgloveApiClientBuilder {
    base_url: String,
    device_token: Option<DeviceToken>,
    user_agent: String,
}

impl Default for FoxgloveApiClientBuilder {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_API_URL.to_string(),
            device_token: None,
            user_agent: default_user_agent(),
        }
    }
}

impl FoxgloveApiClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn device_token(mut self, token: DeviceToken) -> Self {
        self.device_token = Some(token);
        self
    }

    pub fn user_agent(mut self, agent: impl Into<String>) -> Self {
        self.user_agent = agent.into();
        self
    }

    pub fn build(self) -> Result<FoxgloveApiClient, FoxgloveApiClientError> {
        FoxgloveApiClient::new(self.base_url, self.device_token, self.user_agent)
    }
}

#[cfg(test)]
mod test_utils {
    use super::{DeviceResponse, FoxgloveApiClient, FoxgloveApiClientBuilder, RtcCredentials};
    use axum::{extract::Path, http::HeaderMap, Json};
    use axum::{handler::Handler, Router};
    use reqwest::StatusCode;
    use tokio::net::TcpListener;

    pub const TEST_DEVICE_TOKEN: &str = "fox_dt_testtoken";
    pub const TEST_DEVICE_ID: &str = "dev_testdevice";
    pub const TEST_PROJECT_ID: &str = "prj_testproj";

    /// Starts a test server with the given handler mounted at the endpoint.
    /// Returns the base URL (e.g., "http://0.0.0.0:12345").
    pub async fn create_test_endpoint<T: 'static>(
        endpoint: &str,
        handler: impl Handler<T, ()>,
    ) -> String {
        let app = Router::new().route(endpoint, axum::routing::any(handler));

        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        format!("http://{addr}")
    }

    /// Creates a test API client with the handler mounted at the endpoint.
    pub async fn create_test_api_client<T: 'static>(
        endpoint: &str,
        handler: impl Handler<T, ()>,
    ) -> FoxgloveApiClient {
        let url = create_test_endpoint(endpoint, handler).await;
        FoxgloveApiClientBuilder::new()
            .base_url(url)
            .build()
            .unwrap()
    }

    pub async fn device_info_handler(
        headers: HeaderMap,
    ) -> Result<Json<DeviceResponse>, StatusCode> {
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

    pub async fn authorize_remote_viz_handler(
        Path(device_id): Path<String>,
        headers: HeaderMap,
    ) -> Result<Json<RtcCredentials>, StatusCode> {
        let auth = headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        if auth != format!("DeviceToken {TEST_DEVICE_TOKEN}") {
            return Err(StatusCode::UNAUTHORIZED);
        }

        if device_id != TEST_DEVICE_ID {
            return Err(StatusCode::UNAUTHORIZED);
        }

        Ok(Json(RtcCredentials {
            token: "rtc-token-abc123".into(),
            url: "wss://rtc.foxglove.dev".into(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::*;
    use super::*;
    use axum::extract::Path;
    use axum::http::HeaderMap;
    use axum::Json;
    use reqwest::StatusCode;

    #[tokio::test]
    async fn fetch_device_info_requires_token() {
        let client =
            create_test_api_client("/internal/platform/v1/device-info", device_info_handler).await;
        let result = client.fetch_device_info().await;
        assert!(matches!(result, Err(FoxgloveApiClientError::NoToken())));
    }

    #[tokio::test]
    async fn fetch_device_info_success() {
        use crate::api_client::types::DeviceResponse;

        let mut client =
            create_test_api_client("/internal/platform/v1/device-info", device_info_handler).await;
        client.set_device_token(DeviceToken::new(TEST_DEVICE_TOKEN));
        let result = client
            .fetch_device_info()
            .await
            .expect("could not authorize device info");

        assert_eq!(result.id, TEST_DEVICE_ID);
        assert_eq!(result.name, "Test Device");
        assert_eq!(result.project_id, TEST_PROJECT_ID);
        assert_eq!(result.retain_recordings_seconds, Some(3600));
    }

    #[tokio::test]
    async fn fetch_device_info_unauthorized() {
        let mut client =
            create_test_api_client("/internal/platform/v1/device-info", device_info_handler).await;
        client.set_device_token(DeviceToken::new("some-bad-device-token"));
        let result = client.fetch_device_info().await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn authorize_remote_viz_requires_token() {
        let client = create_test_api_client(
            "/internal/platform/v1/devices/:device_id/remote-sessions",
            authorize_remote_viz_handler,
        )
        .await;
        let result = client.authorize_remote_viz(TEST_DEVICE_ID).await;
        assert!(matches!(result, Err(FoxgloveApiClientError::NoToken())));
    }

    #[tokio::test]
    async fn authorize_remote_viz_success() {
        let mut client = create_test_api_client(
            "/internal/platform/v1/devices/:device_id/remote-sessions",
            authorize_remote_viz_handler,
        )
        .await;
        client.set_device_token(DeviceToken::new(TEST_DEVICE_TOKEN));

        let result = client
            .authorize_remote_viz(TEST_DEVICE_ID)
            .await
            .expect("could not authorize remote viz");
        assert_eq!(result.token, "rtc-token-abc123");
        assert_eq!(result.url, "wss://rtc.foxglove.dev");
    }

    #[tokio::test]
    async fn authorize_remote_viz_unauthorized() {
        let mut client = create_test_api_client(
            "/internal/platform/v1/devices/:device_id/remote-sessions",
            authorize_remote_viz_handler,
        )
        .await;
        client.set_device_token(DeviceToken::new("some-bad-device-token"));

        let result = client.authorize_remote_viz(TEST_DEVICE_ID).await;
        assert!(result.is_err());
    }
}

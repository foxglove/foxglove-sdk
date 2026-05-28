use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, USER_AGENT};
use reqwest::{Method, StatusCode};
use thiserror::Error;

use crate::library_version::{get_sdk_language, get_sdk_version};

use super::types::{DeviceResponse, ErrorResponse, WatchHeartbeatRequest, WatchQuery};

const DEFAULT_API_URL: &str = "https://api.foxglove.dev";

const MAX_ERROR_RESPONSE_LEN: u64 = 16_384;

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

    #[error("error response {status} too large")]
    ErrorResponseTooLarge {
        status: StatusCode,
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
}

impl FoxgloveApiClientError {
    pub fn status_code(&self) -> Option<StatusCode> {
        match self {
            Self::Request(
                RequestError::MalformedErrorResponse { status, .. }
                | RequestError::ErrorResponse { status, .. }
                | RequestError::ErrorResponseTooLarge { status, .. },
            ) => Some(*status),
            _ => None,
        }
    }
}

#[must_use]
pub(crate) struct RequestBuilder(reqwest::RequestBuilder);

impl RequestBuilder {
    fn new(client: &reqwest::Client, method: Method, url: &str, user_agent: &str) -> Self {
        Self(client.request(method, url).header(USER_AGENT, user_agent))
    }

    pub fn device_token(mut self, token: &DeviceToken) -> Self {
        self.0 = self.0.header(AUTHORIZATION, token.to_header());
        self
    }

    pub fn accept(mut self, value: &'static str) -> Self {
        self.0 = self.0.header(ACCEPT, value);
        self
    }

    pub fn query<T: serde::Serialize + ?Sized>(mut self, query: &T) -> Self {
        self.0 = self.0.query(query);
        self
    }

    pub fn json<T: serde::Serialize + ?Sized>(mut self, body: &T) -> Self {
        self.0 = self.0.json(body);
        self
    }

    pub async fn send(self) -> Result<reqwest::Response, RequestError> {
        let response = self.0.send().await.map_err(RequestError::SendRequest)?;

        let status = response.status();
        if status.is_client_error() || status.is_server_error() {
            let headers = Box::new(response.headers().clone());
            if response
                .content_length()
                .is_some_and(|len| len > MAX_ERROR_RESPONSE_LEN)
            {
                return Err(RequestError::ErrorResponseTooLarge { status, headers });
            }
            let body = response
                .bytes()
                .await
                .map_err(RequestError::LoadResponseBytes)?;
            if body.len() as u64 > MAX_ERROR_RESPONSE_LEN {
                return Err(RequestError::ErrorResponseTooLarge { status, headers });
            }
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

pub(crate) fn default_user_agent() -> String {
    format!(
        "foxglove-sdk/{} ({})",
        get_sdk_language(),
        get_sdk_version()
    )
}

/// Internal API client for communicating with the Foxglove platform.
///
/// This client is intended for internal use only to support the live visualization feature
/// and is subject to breaking changes at any time. Do not depend on the stability of this type.
#[derive(Clone)]
pub(crate) struct FoxgloveApiClient<A: Clone> {
    http: reqwest::Client,
    http_streaming: reqwest::Client,
    auth: A,
    base_url: String,
    user_agent: String,
}

impl<A: Clone> FoxgloveApiClient<A> {
    fn new(
        base_url: impl Into<String>,
        auth: A,
        user_agent: impl Into<String>,
        timeout_duration: Duration,
    ) -> Result<Self, FoxgloveApiClientError> {
        // Short-request client has a total request timeout applied to every call.
        let http = reqwest::ClientBuilder::new()
            .timeout(timeout_duration)
            .build()?;
        // Streaming client omits the total request timeout (which would also kill long-lived
        // SSE responses) but keeps a connect timeout so that a broken network surfaces quickly.
        let http_streaming = reqwest::ClientBuilder::new()
            .connect_timeout(timeout_duration)
            .build()?;
        Ok(Self {
            http,
            http_streaming,
            auth,
            base_url: base_url.into(),
            user_agent: user_agent.into(),
        })
    }

    fn build_url(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        )
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        RequestBuilder::new(&self.http, method, &self.build_url(path), &self.user_agent)
    }

    fn streaming_request(&self, method: Method, path: &str) -> RequestBuilder {
        RequestBuilder::new(
            &self.http_streaming,
            method,
            &self.build_url(path),
            &self.user_agent,
        )
    }

    pub fn get(&self, endpoint: &str) -> RequestBuilder {
        self.request(Method::GET, endpoint)
    }

    pub fn post(&self, endpoint: &str) -> RequestBuilder {
        self.request(Method::POST, endpoint)
    }

    pub fn stream_get(&self, endpoint: &str) -> RequestBuilder {
        self.streaming_request(Method::GET, endpoint)
    }
}

impl FoxgloveApiClient<DeviceToken> {
    /// Fetches device information from the Foxglove platform.
    ///
    /// This endpoint is not intended for direct usage. Access may be blocked if suspicious
    /// activity is detected.
    pub async fn fetch_device_info(&self) -> Result<DeviceResponse, FoxgloveApiClientError> {
        let response = self
            .get("/internal/platform/v1/device-info")
            .device_token(&self.auth)
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

    /// Opens the long-lived remote access watch SSE stream.
    ///
    /// The caller is responsible for parsing `text/event-stream` frames from the returned
    /// response. The streaming reqwest client does not apply a total-request timeout.
    pub async fn open_watch_stream(
        &self,
        query: &WatchQuery,
    ) -> Result<reqwest::Response, FoxgloveApiClientError> {
        let response = self
            .stream_get("/internal/platform/v1/remote-sessions/watch")
            .device_token(&self.auth)
            .accept("text/event-stream")
            .query(query)
            .send()
            .await?;
        Ok(response)
    }

    /// Refreshes a watch lease by POSTing to the heartbeat endpoint.
    ///
    /// Errors carry the HTTP status code via [`FoxgloveApiClientError::status_code`] so callers
    /// can disambiguate 409 (another gateway holds the lease) and 410 (the supplied lease is
    /// no longer active).
    pub async fn post_watch_heartbeat(
        &self,
        watch_lease_id: &str,
    ) -> Result<(), FoxgloveApiClientError> {
        self.post("/internal/platform/v1/remote-sessions/watch/heartbeat")
            .device_token(&self.auth)
            .json(&WatchHeartbeatRequest { watch_lease_id })
            .send()
            .await?;
        Ok(())
    }
}

pub(crate) struct FoxgloveApiClientBuilder<A> {
    auth: A,
    base_url: String,
    user_agent: String,
    timeout_duration: Duration,
}

impl<A> FoxgloveApiClientBuilder<A> {
    pub fn new(auth: A) -> Self {
        Self {
            auth,
            base_url: DEFAULT_API_URL.to_string(),
            user_agent: default_user_agent(),
            timeout_duration: Duration::from_secs(30),
        }
    }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    pub fn user_agent(mut self, agent: impl Into<String>) -> Self {
        self.user_agent = agent.into();
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout_duration = duration;
        self
    }

    pub fn build(self) -> Result<FoxgloveApiClient<A>, FoxgloveApiClientError>
    where
        A: Clone,
    {
        FoxgloveApiClient::new(
            self.base_url,
            self.auth,
            self.user_agent,
            self.timeout_duration,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::api_client::test_utils::{
        TEST_DEVICE_ID, TEST_DEVICE_TOKEN, TEST_PROJECT_ID, create_test_api_client,
        create_test_server,
    };

    use super::DeviceToken;

    #[tokio::test]
    async fn fetch_device_info_success() {
        let server = create_test_server().await;
        let client = create_test_api_client(server.url(), DeviceToken::new(TEST_DEVICE_TOKEN));
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
        let server = create_test_server().await;
        let client =
            create_test_api_client(server.url(), DeviceToken::new("some-bad-device-token"));
        let result = client.fetch_device_info().await;

        assert!(result.is_err());
    }
}

#![allow(dead_code)]

use std::fmt::Display;
use std::time::Duration;

use percent_encoding::AsciiSet;
use reqwest::header::{HeaderMap, AUTHORIZATION, USER_AGENT};
use reqwest::{Method, StatusCode};
use thiserror::Error;

use super::types::{DeviceResponse, ErrorResponse};

pub const DEFAULT_API_URL: &str = "https://api.foxglove.dev";

const PATH_ENCODING: AsciiSet = percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

pub(crate) fn encode_uri_component(component: &str) -> impl Display + '_ {
    percent_encoding::percent_encode(component.as_bytes(), &PATH_ENCODING)
}

#[derive(Clone)]
pub struct DeviceToken(String);

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
pub enum RequestError {
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
pub enum FoxgloveApiClientError {
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
struct RequestBuilder(reqwest::RequestBuilder);

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
        timeout: Duration,
    ) -> Result<Self, FoxgloveApiClientError> {
        Ok(Self {
            http: reqwest::ClientBuilder::new()
                .pool_idle_timeout(timeout)
                .build()?,
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

    pub fn device_token(&self) -> &Option<DeviceToken> {
        &self.device_token
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
}

impl Default for FoxgloveApiClient {
    fn default() -> Self {
        Self.new(
            DEFAULT_API_URL.to_string(),
            None,
            default_user_agent(),
            Duration::from_secs(30),
        )
    }
}

#![allow(dead_code)]

use std::sync::Arc;

use arc_swap::ArcSwapOption;
use thiserror::Error;

use super::client::FoxgloveApiClientError;
use super::device::Device;

#[derive(Clone)]
pub(crate) struct RtcCredentials {
    pub url: String,
    pub token: String,
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub(crate) enum CredentialsError {
    #[error("failed to fetch credentials: {0}")]
    FetchFailed(#[from] FoxgloveApiClientError),
}

pub(crate) struct CredentialsProvider {
    device: Device,
    credentials: ArcSwapOption<RtcCredentials>,
}

impl CredentialsProvider {
    pub fn new(device: Device) -> Self {
        Self {
            device,
            credentials: ArcSwapOption::new(None),
        }
    }

    #[must_use]
    pub fn current_credentials(&self) -> Option<Arc<RtcCredentials>> {
        self.credentials.load_full()
    }

    pub async fn load_credentials(&self) -> Result<Arc<RtcCredentials>, CredentialsError> {
        if let Some(credentials) = self.current_credentials() {
            return Ok(credentials);
        }

        self.refresh().await
    }

    pub async fn refresh(&self) -> Result<Arc<RtcCredentials>, CredentialsError> {
        let response = self.device.authorize_remote_viz().await?;

        let credentials = Arc::new(RtcCredentials {
            url: response.url,
            token: response.token,
        });

        self.credentials.store(Some(credentials.clone()));
        Ok(credentials)
    }

    pub fn clear(&self) {
        self.credentials.store(None);
    }
}

#![allow(dead_code)]

use std::sync::Arc;

use arc_swap::ArcSwapOption;
use thiserror::Error;
use tokio::sync::Mutex;

use super::client::{
    DeviceToken, FoxgloveApiClient, FoxgloveApiClientBuilder, FoxgloveApiClientError,
};
use super::types::{DeviceResponse, RtcCredentials};

#[derive(Error, Debug)]
#[non_exhaustive]
pub(crate) enum CredentialsError {
    #[error("failed to fetch credentials: {0}")]
    FetchFailed(#[from] FoxgloveApiClientError),
}

pub(crate) struct CredentialsProvider {
    device: DeviceResponse,
    client: FoxgloveApiClient<DeviceToken>,
    credentials: ArcSwapOption<RtcCredentials>,
    refresh_lock: Mutex<()>,
}

impl CredentialsProvider {
    pub async fn new(
        client_builder: FoxgloveApiClientBuilder<DeviceToken>,
    ) -> Result<Self, FoxgloveApiClientError> {
        let client = client_builder.build()?;
        let device = client.fetch_device_info().await?;
        Ok(Self {
            device,
            client,
            credentials: ArcSwapOption::new(None),
            refresh_lock: Mutex::new(()),
        })
    }

    #[must_use]
    pub fn current_credentials(&self) -> Option<Arc<RtcCredentials>> {
        self.credentials.load_full()
    }

    pub async fn load_credentials(&self) -> Result<Arc<RtcCredentials>, CredentialsError> {
        if let Some(credentials) = self.current_credentials() {
            return Ok(credentials);
        }

        let _refresh_guard = self.refresh_lock.lock().await;
        if let Some(credentials) = self.current_credentials() {
            return Ok(credentials);
        }

        self.refresh().await
    }

    async fn refresh(&self) -> Result<Arc<RtcCredentials>, CredentialsError> {
        let _refresh_guard = self.refresh_lock.lock().await;
        let credentials = Arc::new(self.client.authorize_remote_viz(&self.device.id).await?);
        self.credentials.store(Some(credentials.clone()));
        Ok(credentials)
    }

    pub fn clear(&self) {
        self.credentials.store(None);
    }
}

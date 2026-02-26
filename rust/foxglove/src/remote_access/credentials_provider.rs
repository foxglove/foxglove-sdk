#![allow(dead_code)]

use std::sync::Arc;

use arc_swap::ArcSwapOption;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::api_client::{
    DeviceResponse, DeviceToken, FoxgloveApiClient, FoxgloveApiClientBuilder,
    FoxgloveApiClientError, RtcCredentials,
};

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

        tracing::info!("refreshing credentials");
        let credentials = Arc::new(self.client.authorize_remote_viz(&self.device.id).await?);
        self.credentials.store(Some(credentials.clone()));
        Ok(credentials)
    }

    pub fn device_name(&self) -> &str {
        &self.device.name
    }

    pub async fn clear(&self) {
        let _refresh_guard = self.refresh_lock.lock().await;
        self.credentials.store(None);
    }
}

#[cfg(test)]
mod tests {
    use crate::api_client::test_utils::{
        TEST_DEVICE_TOKEN, create_test_builder, create_test_server,
    };

    use crate::api_client::DeviceToken;

    use super::CredentialsProvider;

    #[tokio::test]
    async fn new_succeeds_with_no_cached_credentials() {
        let server = create_test_server().await;
        let builder = create_test_builder(server.url(), DeviceToken::new(TEST_DEVICE_TOKEN));

        let provider = CredentialsProvider::new(builder)
            .await
            .expect("should construct successfully");

        assert!(provider.current_credentials().is_none());
    }

    #[tokio::test]
    async fn new_fails_with_bad_token() {
        let server = create_test_server().await;
        let builder = create_test_builder(server.url(), DeviceToken::new("bad-token"));

        let result = CredentialsProvider::new(builder).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn load_credentials_fetches_and_caches() {
        let server = create_test_server().await;
        let builder = create_test_builder(server.url(), DeviceToken::new(TEST_DEVICE_TOKEN));
        let provider = CredentialsProvider::new(builder).await.unwrap();

        let credentials = provider
            .load_credentials()
            .await
            .expect("should fetch credentials");

        assert_eq!(credentials.token, "rtc-token-abc123");
        assert_eq!(credentials.url, "wss://rtc.foxglove.dev");
        assert!(provider.current_credentials().is_some());
    }

    #[tokio::test]
    async fn clear_removes_cached_credentials() {
        let server = create_test_server().await;
        let builder = create_test_builder(server.url(), DeviceToken::new(TEST_DEVICE_TOKEN));
        let provider = CredentialsProvider::new(builder).await.unwrap();

        provider.load_credentials().await.unwrap();
        assert!(provider.current_credentials().is_some());

        provider.clear().await;
        assert!(provider.current_credentials().is_none());
    }
}

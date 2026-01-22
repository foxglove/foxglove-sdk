#![allow(dead_code)]

use super::client::{
    default_user_agent, encode_uri_component, DeviceToken, FoxgloveApiClient,
    FoxgloveApiClientError,
};
use super::types::{AuthorizeRemoteVizResponse, DeviceResponse};
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct Device {
    info: DeviceResponse,
    client: FoxgloveApiClient,
}

impl Device {
    pub async fn new(token: DeviceToken) -> Result<Self, FoxgloveApiClientError> {
        let mut client = FoxgloveApiClient::default();
        client.set_device_token(token);
        let device_info = client.fetch_device_info().await?;
        Ok(Self {
            client,
            info: device_info,
        })
    }

    pub fn id(&self) -> &str {
        &self.info.id
    }

    pub fn name(&self) -> &str {
        &self.info.name
    }

    pub fn project_id(&self) -> &str {
        &self.info.project_id
    }

    pub fn info(&self) -> &DeviceResponse {
        &self.info
    }

    pub async fn authorize_remote_viz(
        &self,
    ) -> Result<AuthorizeRemoteVizResponse, FoxgloveApiClientError> {
        let Some(device_token) = self.client.device_token() else {
            return Err(FoxgloveApiClientError::NoToken());
        };

        let device_id = encode_uri_component(&self.info.id);
        let response = self
            .client
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

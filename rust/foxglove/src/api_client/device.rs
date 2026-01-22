#![allow(dead_code)]

use super::client::{
    DeviceToken, FoxgloveApiClient, FoxgloveApiClientBuilder, FoxgloveApiClientError,
};
use super::types::{AuthorizeRemoteVizResponse, DeviceResponse};

#[derive(Clone)]
pub(crate) struct Device {
    info: DeviceResponse,
    client: FoxgloveApiClient,
}

impl Device {
    pub async fn new(token: DeviceToken) -> Result<Self, FoxgloveApiClientError> {
        let client = FoxgloveApiClientBuilder::default()
            .device_token(token)
            .build()?;

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
        self.client.authorize_remote_viz(self.id()).await
    }
}

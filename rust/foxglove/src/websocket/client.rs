use std::sync::Weak;

use super::Status;
use super::connected_client::ConnectedClient;
use crate::SinkId;
pub use crate::remote_common::ClientId;
use crate::remote_common::fetch_asset::SendAssetResponse;

/// A connected client session with the websocket server.
#[derive(Debug, Clone)]
pub struct Client {
    id: ClientId,
    sink_id: SinkId,
    client: Weak<ConnectedClient>,
}

impl Client {
    pub(super) fn new(client: &ConnectedClient) -> Self {
        Self {
            id: client.id(),
            sink_id: client.sink_id(),
            client: client.weak().clone(),
        }
    }

    /// Returns the client ID.
    pub fn id(&self) -> ClientId {
        self.id
    }

    /// Returns the client's sink ID.
    pub fn sink_id(&self) -> Option<SinkId> {
        Some(self.sink_id)
    }

    /// Send a status message to this client. Does nothing if client is disconnected.
    pub fn send_status(&self, status: Status) {
        if let Some(client) = self.client.upgrade() {
            client.send_status(status);
        }
    }
}

impl SendAssetResponse for Client {
    fn send_asset_response(&self, result: Result<&[u8], &str>, request_id: u32) {
        if let Some(client) = self.client.upgrade() {
            match result {
                Ok(asset) => client.send_asset_response(asset, request_id),
                Err(err) => client.send_asset_error(err, request_id),
            }
        }
    }
}

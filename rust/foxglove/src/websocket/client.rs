use std::sync::Weak;

use super::Status;
use super::connected_client::ConnectedClient;
use crate::SinkId;
pub use crate::remote_common::ClientId;

/// A connected client session with the websocket server.
#[derive(Debug, Clone)]
pub struct Client {
    id: ClientId,
    client: Weak<ConnectedClient>,
}

impl Client {
    pub(super) fn new(client: &ConnectedClient) -> Self {
        Self {
            id: client.id(),
            client: client.weak().clone(),
        }
    }

    /// Returns the client ID.
    pub fn id(&self) -> ClientId {
        self.id
    }

    /// Returns the client's sink ID
    pub fn sink_id(&self) -> Option<SinkId> {
        self.client.upgrade().map(|client| client.sink_id())
    }

    /// Send a status message to this client. Does nothing if client is disconnected.
    pub fn send_status(&self, status: Status) {
        if let Some(client) = self.client.upgrade() {
            client.send_status(status);
        }
    }
}

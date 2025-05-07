use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Weak;

use super::connected_client::ConnectedClient;
use super::Status;

/// Identifies a client connection. Unique for the duration of the server's lifetime.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ClientId(u32);

impl ClientId {
    /// Allocates the next client ID.
    pub(crate) fn next() -> Self {
        static NEXT_ID: AtomicU32 = AtomicU32::new(1);
        let id = NEXT_ID.fetch_add(1, Relaxed);
        Self(id)
    }
}

impl From<ClientId> for u32 {
    fn from(client: ClientId) -> Self {
        client.0
    }
}

impl std::fmt::Display for ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

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

    /// Send a status message to this client. Does nothing if client is disconnected.
    pub fn send_status(&self, status: Status) {
        if let Some(client) = self.client.upgrade() {
            client.send_status(status);
        }
    }

    /// Send a fetch asset response to the client. Does nothing if client is disconnected.
    pub(crate) fn send_asset_response(&self, result: Result<&[u8], &str>, request_id: u32) {
        if let Some(client) = self.client.upgrade() {
            match result {
                Ok(asset) => client.send_asset_response(asset, request_id),
                Err(err) => client.send_asset_error(err, request_id),
            }
        }
    }
}

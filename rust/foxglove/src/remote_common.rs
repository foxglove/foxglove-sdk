//! Common types shared between the websocket and remote access modules.

use std::sync::atomic::{AtomicU32, Ordering};

pub(crate) mod connection_graph;
pub(crate) mod semaphore;
pub(crate) mod service;

/// Identifies a client connection. Unique for the duration of the server's lifetime.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ClientId(pub(crate) u32);

impl ClientId {
    pub(crate) fn next() -> Self {
        static NEXT_ID: AtomicU32 = AtomicU32::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        assert_ne!(id, 0, "ClientId overflowed");
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

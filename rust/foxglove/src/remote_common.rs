//! Types shared between the WebSocket server and the remote-access gateway.
//!
//! Everything in this module is transport-agnostic: handler traits ([`ParameterHandler`],
//! [`AssetHandler`], [`service::Handler`]) implemented against these types can be registered with
//! either the WebSocket server or the remote-access gateway, and data and identity types
//! ([`Parameter`], [`Status`], [`ConnectionGraph`], [`ClientId`], [`service::Service`], ...) are
//! the same concrete types used by both. The transport-specific [`crate::websocket`] and
//! [`crate::remote_access`] modules re-export the same items for convenience.

use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(any(feature = "websocket", feature = "remote-access"))]
pub(crate) mod any_client;
pub(crate) mod connection_graph;
#[cfg(any(feature = "websocket", feature = "remote-access"))]
pub(crate) mod fetch_asset;
#[cfg(any(feature = "websocket", feature = "remote-access"))]
pub(crate) mod parameters;
pub(crate) mod semaphore;
pub mod service;

#[cfg(any(feature = "websocket", feature = "remote-access"))]
pub use any_client::AnyClient;
pub use connection_graph::ConnectionGraph;
#[cfg(any(feature = "websocket", feature = "remote-access"))]
pub use fetch_asset::{AssetHandler, AssetResponder};
#[cfg(any(feature = "websocket", feature = "remote-access"))]
pub use parameters::{GetParametersResponder, ParameterHandler, SetParametersResponder};

pub use crate::protocol::common::parameter::{
    DecodeError as ParameterDecodeError, Parameter, ParameterType, ParameterValue,
};
pub use crate::protocol::common::server::status::{Level as StatusLevel, Status};

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

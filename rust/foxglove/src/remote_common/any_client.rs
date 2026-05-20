//! Transport-agnostic client handle for handler traits.

use crate::protocol::common::parameter::Parameter;
use crate::protocol::common::server::status::Status;
use crate::remote_common::ClientId;
use crate::remote_common::fetch_asset::SendAssetResponse;
use crate::remote_common::parameters::SendParameterResponse;

/// A client handle abstracted over the transport that delivered the request.
#[derive(Debug, Clone)]
pub struct AnyClient(AnyClientInner);

#[derive(Debug, Clone)]
enum AnyClientInner {
    #[cfg(feature = "websocket")]
    WebSocket(crate::websocket::Client),
    #[cfg(feature = "remote-access")]
    RemoteAccess(crate::remote_access::Client),
}

impl AnyClient {
    /// The transport-assigned client ID.
    pub fn id(&self) -> ClientId {
        match &self.0 {
            #[cfg(feature = "websocket")]
            AnyClientInner::WebSocket(c) => c.id(),
            #[cfg(feature = "remote-access")]
            AnyClientInner::RemoteAccess(c) => c.id(),
        }
    }

    /// Send a status message to this client.
    fn send_status(&self, status: Status) {
        match &self.0 {
            #[cfg(feature = "websocket")]
            AnyClientInner::WebSocket(c) => c.send_status(status),
            #[cfg(feature = "remote-access")]
            AnyClientInner::RemoteAccess(c) => c.send_status(status),
        }
    }

    /// Send a generic error status to this client.
    pub fn send_error(&self, message: impl Into<String>) {
        self.send_status(Status::error(message));
    }

    /// Send a generic warning status to this client.
    pub fn send_warning(&self, message: impl Into<String>) {
        self.send_status(Status::warning(message));
    }

    /// Send a generic info status to this client.
    pub fn send_info(&self, message: impl Into<String>) {
        self.send_status(Status::info(message));
    }

    /// If this client is a WebSocket client, returns a reference to it.
    #[cfg(feature = "websocket")]
    pub fn as_websocket(&self) -> Option<&crate::websocket::Client> {
        match &self.0 {
            AnyClientInner::WebSocket(c) => Some(c),
            #[cfg(feature = "remote-access")]
            AnyClientInner::RemoteAccess(_) => None,
        }
    }

    /// If this client is a remote-access participant, returns a reference to it.
    #[cfg(feature = "remote-access")]
    pub fn as_remote_access(&self) -> Option<&crate::remote_access::Client> {
        match &self.0 {
            #[cfg(feature = "websocket")]
            AnyClientInner::WebSocket(_) => None,
            AnyClientInner::RemoteAccess(c) => Some(c),
        }
    }

    #[cfg(feature = "websocket")]
    pub(crate) fn from_websocket(client: crate::websocket::Client) -> Self {
        Self(AnyClientInner::WebSocket(client))
    }

    #[cfg(feature = "remote-access")]
    pub(crate) fn from_remote_access(client: crate::remote_access::Client) -> Self {
        Self(AnyClientInner::RemoteAccess(client))
    }

    pub(crate) fn send_parameter_values(
        &self,
        parameters: Vec<Parameter>,
        request_id: Option<String>,
    ) {
        match &self.0 {
            #[cfg(feature = "websocket")]
            AnyClientInner::WebSocket(c) => c.send_parameter_values(parameters, request_id),
            #[cfg(feature = "remote-access")]
            AnyClientInner::RemoteAccess(c) => c.send_parameter_values(parameters, request_id),
        }
    }

    pub(crate) fn send_asset_response(&self, result: Result<&[u8], &str>, request_id: u32) {
        match &self.0 {
            #[cfg(feature = "websocket")]
            AnyClientInner::WebSocket(c) => c.send_asset_response(result, request_id),
            #[cfg(feature = "remote-access")]
            AnyClientInner::RemoteAccess(c) => c.send_asset_response(result, request_id),
        }
    }
}

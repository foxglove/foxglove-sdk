//! Websocket service request.

use std::sync::Arc;

use bytes::Bytes;

use crate::websocket::ClientId;

use super::{CallId, Service};

/// A service call request.
#[derive(Clone)]
pub struct Request {
    service: Arc<Service>,
    client_id: ClientId,
    call_id: CallId,
    encoding: String,
    payload: Bytes,
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("service", &self.service)
            .field("client_id", &self.client_id)
            .field("call_id", &self.call_id)
            .field("encoding", &self.encoding)
            .finish_non_exhaustive()
    }
}

impl Request {
    /// Constructs a new request.
    pub(crate) fn new(
        service: Arc<Service>,
        client_id: ClientId,
        call_id: CallId,
        encoding: String,
        payload: Bytes,
    ) -> Self {
        Self {
            service,
            client_id,
            call_id,
            encoding,
            payload,
        }
    }

    /// The service name.
    pub fn service_name(&self) -> &str {
        self.service.name()
    }

    /// The client ID.
    pub fn client_id(&self) -> ClientId {
        self.client_id
    }

    /// The call ID that uniquely identifies this request for this client.
    pub fn call_id(&self) -> CallId {
        self.call_id
    }

    /// The request encoding.
    pub fn encoding(&self) -> &str {
        &self.encoding
    }

    /// A reference to the request payload.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Consumes the request to return the inner payload.
    pub fn into_payload(self) -> Bytes {
        self.payload
    }
}

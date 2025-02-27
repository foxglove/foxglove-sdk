//! Service call response handling.

use std::sync::Arc;

use bytes::Bytes;
use tokio_tungstenite::tungstenite::Message;

use super::{CallId, ServiceId};
use crate::websocket::{protocol, ConnectedClient, SemaphoreGuard};

/// A handle for completing a service call.
///
/// If you're holding one of these, you're responsible for eventually calling
/// [`Responder::respond`]. If you drop the responder without responding, the client will never
/// receive a response for its request.
#[must_use]
pub struct Responder {
    client: Arc<ConnectedClient>,
    service_id: ServiceId,
    call_id: CallId,
    encoding: String,
    _guard: SemaphoreGuard,
}
impl Responder {
    /// Creates a new responder.
    pub(crate) fn new(
        client: Arc<ConnectedClient>,
        service_id: ServiceId,
        call_id: CallId,
        encoding: impl Into<String>,
        _guard: SemaphoreGuard,
    ) -> Self {
        Self {
            client,
            service_id,
            call_id,
            encoding: encoding.into(),
            _guard,
        }
    }

    /// Overrides the default response encoding.
    ///
    /// By default, the response encoding is the one declared in the
    /// [`ServiceSchema`][super::ServiceSchema]. If no response encoding was declared, then the
    /// encoding is presumed to be the same as the request.
    pub fn set_encoding(&mut self, encoding: impl Into<String>) {
        self.encoding = encoding.into();
    }

    /// Completes the request by sending a response to the client.
    pub fn respond(self, result: Result<Bytes, String>) {
        let message = match result {
            Ok(payload) => Message::binary(
                protocol::server::ServiceCallResponse::new(
                    self.service_id,
                    self.call_id,
                    self.encoding,
                    payload,
                )
                .encode(),
            ),
            Err(message) => Message::text(protocol::server::service_call_failure(
                self.service_id,
                self.call_id,
                &message,
            )),
        };

        // Callee logs errors.
        let _ = self.client.send_control_msg(message);
    }
}

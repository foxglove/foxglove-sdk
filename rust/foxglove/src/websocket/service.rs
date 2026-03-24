//! Websocket services.

use std::borrow::Cow;
use std::sync::Arc;

use tokio_tungstenite::tungstenite::Message;

use crate::websocket::connected_client::ConnectedClient;
use crate::websocket::semaphore::SemaphoreGuard;
use crate::websocket::ws_protocol::server::{ServiceCallFailure, ServiceCallResponse};

// Re-export all public types from the common service module.
pub use crate::remote_common::service::{
    CallId, ClientId, Handler, Request, Responder, Service, ServiceBuilder, ServiceSchema,
    SyncHandler,
};
pub(crate) use crate::remote_common::service::{ServiceId, ServiceMap};

/// Sends service call responses over a WebSocket connection.
struct ResponseSender {
    client: Arc<ConnectedClient>,
    service_id: ServiceId,
    call_id: CallId,
    _guard: SemaphoreGuard,
}

impl crate::remote_common::service::ResponseSender for ResponseSender {
    fn send(&mut self, result: Result<(&str, &[u8]), String>) {
        let message = match result {
            Ok((encoding, payload)) => Message::from(&ServiceCallResponse {
                service_id: self.service_id.into(),
                call_id: self.call_id.into(),
                encoding: encoding.into(),
                payload: Cow::Borrowed(payload),
            }),
            Err(message) => Message::from(&ServiceCallFailure {
                service_id: self.service_id.into(),
                call_id: self.call_id.into(),
                message,
            }),
        };

        // Callee logs errors.
        let _ = self.client.send_control_msg(message);
    }
}

/// Creates a new [`Responder`] backed by a WebSocket connection.
pub(in crate::websocket) fn new_responder(
    client: Arc<ConnectedClient>,
    service_id: ServiceId,
    call_id: CallId,
    encoding: impl Into<String>,
    guard: SemaphoreGuard,
) -> Responder {
    let sender = Box::new(ResponseSender {
        client,
        service_id,
        call_id,
        _guard: guard,
    });
    Responder::new(encoding, sender)
}

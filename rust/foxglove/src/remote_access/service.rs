//! Remote access services.

use std::borrow::Cow;
use std::sync::Arc;

use crate::protocol::v2::server::{ServiceCallFailure, ServiceCallResponse};
use crate::remote_access::participant::Participant;
use crate::remote_access::session::ControlPlaneMessage;
use crate::remote_common::semaphore::SemaphoreGuard;
use crate::remote_common::service::ServiceId;

// Re-export service types so Gateway::services() callers can construct services.
pub use crate::remote_common::service::{
    CallId, Handler, Request, Responder, Service, ServiceBuilder, ServiceSchema, SyncHandler,
};

/// Sends service call responses over the remote access control plane.
struct ResponseSender {
    participant: Arc<Participant>,
    service_id: ServiceId,
    call_id: CallId,
    control_plane_tx: flume::Sender<ControlPlaneMessage>,
    _guard: SemaphoreGuard,
}

impl crate::remote_common::service::ResponseSender for ResponseSender {
    fn send(&mut self, result: Result<(&str, &[u8]), String>) {
        let msg = match result {
            Ok((encoding, payload)) => {
                let response = ServiceCallResponse {
                    service_id: self.service_id.into(),
                    call_id: self.call_id.into(),
                    encoding: encoding.into(),
                    payload: Cow::Borrowed(payload),
                };
                ControlPlaneMessage::binary(self.participant.clone(), &response)
            }
            Err(message) => {
                let failure = ServiceCallFailure {
                    service_id: self.service_id.into(),
                    call_id: self.call_id.into(),
                    message,
                };
                ControlPlaneMessage::json(self.participant.clone(), &failure)
            }
        };
        if let Err(e) = self.control_plane_tx.send(msg) {
            tracing::warn!("control plane queue disconnected, dropping service response: {e}");
        }
    }
}

/// Creates a new [`Responder`] backed by a remote access control plane.
pub(super) fn new_responder(
    participant: Arc<Participant>,
    service_id: ServiceId,
    call_id: CallId,
    encoding: impl Into<String>,
    control_plane_tx: flume::Sender<ControlPlaneMessage>,
    guard: SemaphoreGuard,
) -> Responder {
    let sender = Box::new(ResponseSender {
        participant,
        service_id,
        call_id,
        control_plane_tx,
        _guard: guard,
    });
    Responder::new(encoding, sender)
}

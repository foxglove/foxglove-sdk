//! Remote access services.

use std::borrow::Cow;
use std::sync::{Arc, Weak};

use crate::protocol::v2::server::{ServiceCallFailure, ServiceCallResponse};
use crate::remote_access::participant::Participant;
use crate::remote_access::session::{encode_binary_message, encode_json_message};
use crate::remote_common::semaphore::SemaphoreGuard;
use crate::remote_common::service::ServiceId;

// Re-export service types so Gateway::services() callers can construct services.
pub use crate::remote_common::service::{
    CallId, Handler, Request, Responder, Service, ServiceBuilder, ServiceSchema, SyncHandler,
};

/// Sends service call responses over the remote access control plane.
///
/// Holds a `Weak<Participant>` so that in-flight service calls do not extend the
/// participant's lifetime past removal. If the participant has been removed by
/// the time the handler responds, the response is logged and dropped.
struct ResponseSender {
    participant: Weak<Participant>,
    service_id: ServiceId,
    call_id: CallId,
    _guard: SemaphoreGuard,
}

impl crate::remote_common::service::ResponseSender for ResponseSender {
    fn send(&mut self, result: Result<(&str, &[u8]), String>) {
        let Some(participant) = self.participant.upgrade() else {
            tracing::debug!(
                service_id = ?self.service_id,
                call_id = ?self.call_id,
                "participant disconnected, dropping service response",
            );
            return;
        };
        let data = match result {
            Ok((encoding, payload)) => {
                let response = ServiceCallResponse {
                    service_id: self.service_id.into(),
                    call_id: self.call_id.into(),
                    encoding: encoding.into(),
                    payload: Cow::Borrowed(payload),
                };
                encode_binary_message(&response)
            }
            Err(message) => {
                let failure = ServiceCallFailure {
                    service_id: self.service_id.into(),
                    call_id: self.call_id.into(),
                    message,
                };
                encode_json_message(&failure)
            }
        };
        participant.try_queue_control(data);
    }
}

/// Creates a new [`Responder`] backed by a remote access control plane.
pub(super) fn new_responder(
    participant: &Arc<Participant>,
    service_id: ServiceId,
    call_id: CallId,
    encoding: impl Into<String>,
    guard: SemaphoreGuard,
) -> Responder {
    let sender = Box::new(ResponseSender {
        participant: Arc::downgrade(participant),
        service_id,
        call_id,
        _guard: guard,
    });
    Responder::new(encoding, sender)
}

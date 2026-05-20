use std::sync::{Arc, Weak};

use livekit::id::ParticipantIdentity;

use crate::protocol::common::parameter::Parameter;
use crate::protocol::common::server::ParameterValues;
use crate::protocol::common::server::status::Status;
use crate::remote_access::participant::Participant;
use crate::remote_access::session::encode_json_message;
use crate::remote_common::ClientId;
use crate::remote_common::fetch_asset::SendAssetResponse;
use crate::remote_common::parameters::SendParameterResponse;

/// Represents a connected remote access client.
#[derive(Debug, Clone)]
pub struct Client {
    /// Locally-significant identifier for this particular instance of this participant.
    client_id: ClientId,
    /// LiveKit participant ID.
    participant_id: ParticipantIdentity,
    /// Weak reference to the participant, used for sending asset/parameter responses.
    participant: Option<Weak<Participant>>,
}

impl Client {
    /// Instantiate a new client.
    pub(super) fn new(client_id: ClientId, participant_id: ParticipantIdentity) -> Self {
        Self {
            client_id,
            participant_id,
            participant: None,
        }
    }

    /// Instantiate a new client capable of sending asset/parameter responses.
    pub(super) fn with_sender(
        client_id: ClientId,
        participant_id: ParticipantIdentity,
        participant: &Arc<Participant>,
    ) -> Self {
        Self {
            client_id,
            participant_id,
            participant: Some(Arc::downgrade(participant)),
        }
    }

    /// Returns the locally-significant client ID.
    pub fn id(&self) -> ClientId {
        self.client_id
    }

    /// Returns the client-provided identity.
    ///
    /// This is public for testing purposes, but not intended for end-users.
    #[doc(hidden)]
    pub fn participant_id(&self) -> &str {
        &self.participant_id.0
    }

    /// Send a status message to this client. Does nothing if the client has no sender or if the
    /// participant has been dropped.
    pub(crate) fn send_status(&self, status: Status) {
        let Some(participant) = self.participant.as_ref().and_then(|w| w.upgrade()) else {
            tracing::debug!(
                client_id = ?self.client_id,
                participant_id = ?self.participant_id,
                "participant disconnected, dropping status message"
            );
            return;
        };
        participant.send_control(encode_json_message(&status));
    }
}

impl SendAssetResponse for Client {
    /// Send a fetch asset response to the client.
    /// Does nothing if the client has no sender or if the participant has been dropped.
    fn send_asset_response(&self, result: Result<&[u8], &str>, request_id: u32) {
        let Some(weak) = &self.participant else {
            tracing::debug!(
                client_id = ?self.client_id,
                participant_id = ?self.participant_id,
                request_id,
                "send_asset_response called but participant is not set"
            );
            return;
        };
        let Some(participant) = weak.upgrade() else {
            tracing::debug!(
                client_id = ?self.client_id,
                participant_id = ?self.participant_id,
                request_id,
                "participant disconnected, dropping asset response"
            );
            return;
        };
        match result {
            Ok(data) => participant.send_asset_response(data, request_id),
            Err(err) => participant.send_asset_error(err, request_id),
        }
    }
}

impl SendParameterResponse for Client {
    fn send_parameter_values(&self, parameters: Vec<Parameter>, request_id: Option<String>) {
        let Some(participant) = self.participant.as_ref().and_then(|w| w.upgrade()) else {
            tracing::debug!(
                client_id = ?self.client_id,
                participant_id = ?self.participant_id,
                "participant disconnected, dropping parameter values response"
            );
            return;
        };
        let mut msg = ParameterValues::new(parameters.into_iter().filter(|p| p.value.is_some()));
        if let Some(id) = request_id {
            msg = msg.with_id(id);
        }
        participant.send_control(encode_json_message(&msg));
    }
}

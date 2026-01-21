use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use arc_swap::ArcSwapOption;
use bimap::BiHashMap;
use livekit::{id::ParticipantIdentity, Room, RoomEvent, RoomOptions};
use parking_lot::RwLock;
use tokio::{runtime::Handle, sync::mpsc::UnboundedReceiver};

use crate::{
    cloud::{
        participant::{Participant, ParticipantWriter},
        CloudError,
    },
    websocket::{self, Server},
    CloudSinkListener, SinkChannelFilter,
};

type Result<T> = std::result::Result<T, CloudError>;

// TODO placeholder until auth is implemented, we'll import this from there instead
/// Credentials to access the remote visualization server.
pub struct RtcCredentials {
    /// URL of the RTC server where these credentials are valid.
    pub url: String,
    /// Expiring access token (JWT)
    pub token: String,
}

impl RtcCredentials {
    pub fn new() -> Self {
        Self {
            url: std::env::var("LIVEKIT_HOST").expect("LIVEKIT_HOST must be set"),
            token: std::env::var("LIVEKIT_TOKEN").expect("LIVEKIT_TOKEN must be set"),
        }
    }
}

#[derive(Clone)]
pub(crate) struct CloudConnectionOptions {
    pub session_id: String,
    pub listener: Option<Arc<dyn CloudSinkListener>>,
    pub capabilities: Vec<websocket::Capability>,
    pub supported_encodings: Option<HashSet<String>>,
    pub runtime: Option<Handle>,
    pub channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    pub server_info: Option<HashMap<String, String>>,
}

impl Default for CloudConnectionOptions {
    fn default() -> Self {
        Self {
            session_id: Server::generate_session_id(),
            listener: None,
            capabilities: Vec::new(),
            supported_encodings: None,
            runtime: None,
            channel_filter: None,
            server_info: None,
        }
    }
}

impl std::fmt::Debug for CloudConnectionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudConnectionOptions")
            .field("session_id", &self.session_id)
            .field("has_listener", &self.listener.is_some())
            .field("capabilities", &self.capabilities)
            .field("supported_encodings", &self.supported_encodings)
            .field("has_runtime", &self.runtime.is_some())
            .field("has_channel_filter", &self.channel_filter.is_some())
            .field("server_info", &self.server_info)
            .finish()
    }
}

struct CloudSession {
    credentials: RtcCredentials,
    room: Room,
    room_events: UnboundedReceiver<RoomEvent>,
}

pub(crate) struct CloudConnection {
    options: CloudConnectionOptions,
    participants: RwLock<HashMap<ParticipantIdentity, Arc<Participant>>>,
    session: ArcSwapOption<CloudSession>,
}

impl CloudConnection {
    pub fn new(options: CloudConnectionOptions) -> Self {
        Self {
            options,
            participants: RwLock::new(HashMap::new()),
            session: ArcSwapOption::new(None),
        }
    }

    pub(crate) async fn connect_session(&self) -> Result<()> {
        // TODO get credentials from API
        let credentials = RtcCredentials::new();

        let session =
            match Room::connect(&credentials.url, &credentials.token, RoomOptions::default()).await
            {
                Ok((room, room_events)) => Arc::new(CloudSession {
                    credentials,
                    room,
                    room_events,
                }),
                Err(e) => {
                    return Err(e.into());
                }
            };
        self.session.store(Some(session));

        Ok(())
    }
}

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use arc_swap::ArcSwapOption;
use bimap::BiHashMap;
use livekit::{id::ParticipantIdentity, Room};
use parking_lot::RwLock;
use tokio::runtime::Handle;

use crate::{
    cloud::participant::{Participant, ParticipantWriter},
    websocket::{self, Server},
    CloudSinkListener, SinkChannelFilter,
};

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

pub(crate) struct CloudConnection {
    options: CloudConnectionOptions,
    participants: RwLock<HashMap<ParticipantIdentity, Arc<Participant>>>,
    room: ArcSwapOption<Room>,
}

impl CloudConnection {
    pub fn new(options: CloudConnectionOptions) -> Self {
        Self {
            options,
            participants: RwLock::new(HashMap::new()),
            room: ArcSwapOption::new(None),
        }
    }
}

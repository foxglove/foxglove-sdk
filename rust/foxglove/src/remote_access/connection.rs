use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use livekit::{id::ParticipantIdentity, Room, RoomEvent, RoomOptions, StreamByteOptions};
use parking_lot::RwLock;
use smallvec::SmallVec;
use tokio::{runtime::Handle, sync::mpsc::UnboundedReceiver, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    library_version::get_library_version,
    remote_access::{
        participant::{Participant, ParticipantWriter},
        RemoteAccessError,
    },
    websocket::{self, Server},
    Context, RemoteAccessSinkListener, SinkChannelFilter,
};

use crate::protocol::v2::{server::ServerInfo, JsonMessage};

type Result<T> = std::result::Result<T, RemoteAccessError>;

const WS_PROTOCOL_TOPIC: &str = "ws-protocol";
const AUTH_RETRY_PERIOD: Duration = Duration::from_secs(30);

/// The operation code for the message framing for protocol v2.
/// Distinguishes between frames containing JSON messages vs binary messages.
#[derive(Clone, Copy, Debug)]
#[repr(u8)]
enum OpCode {
    /// The frame contains a JSON message.
    Text = 1,
    /// The frame contains a binary message.
    // TODO future use
    #[expect(dead_code)]
    Binary = 2,
}

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

/// Options for the remote access connection.
///
/// This should be constructed from the [`crate::RemoteAccessSink`] builder.
#[derive(Clone)]
pub(crate) struct RemoteAccessConnectionOptions {
    pub session_id: String,
    pub listener: Option<Arc<dyn RemoteAccessSinkListener>>,
    pub capabilities: Vec<websocket::Capability>,
    pub supported_encodings: Option<HashSet<String>>,
    pub runtime: Option<Handle>,
    pub channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    pub server_info: Option<HashMap<String, String>>,
    pub cancellation_token: CancellationToken,
    pub context: Arc<Context>,
}

impl Default for RemoteAccessConnectionOptions {
    fn default() -> Self {
        Self {
            session_id: Server::generate_session_id(),
            listener: None,
            capabilities: Vec::new(),
            supported_encodings: None,
            runtime: None,
            channel_filter: None,
            server_info: None,
            cancellation_token: CancellationToken::new(),
            context: Context::get_default(),
        }
    }
}

impl std::fmt::Debug for RemoteAccessConnectionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteAccessConnectionOptions")
            .field("session_id", &self.session_id)
            .field("has_listener", &self.listener.is_some())
            .field("capabilities", &self.capabilities)
            .field("supported_encodings", &self.supported_encodings)
            .field("has_runtime", &self.runtime.is_some())
            .field("has_channel_filter", &self.channel_filter.is_some())
            .field("server_info", &self.server_info)
            .field("context", &self.context)
            .finish()
    }
}

/// RemoteAccessSession tracks a connected LiveKit session (the Room)
/// and any state that is specific to that session.
/// We discard this state if we close or lose the connection.
/// [`RemoteAccessConnection`] manages the current connected session (if any)
struct RemoteAccessSession {
    room: Room,
    participants: RwLock<HashMap<ParticipantIdentity, Arc<Participant>>>,
}

/// RemoteAccessConnection manages the connected [`RemoteAccessSession`] to the LiveKit server,
/// and holds the options and other state that outlive a session.
pub(crate) struct RemoteAccessConnection {
    options: RemoteAccessConnectionOptions,
}

impl RemoteAccessConnection {
    pub fn new(options: RemoteAccessConnectionOptions) -> Self {
        Self { options }
    }

    async fn connect_session(
        &self,
    ) -> Result<(Arc<RemoteAccessSession>, UnboundedReceiver<RoomEvent>)> {
        // TODO get credentials from API
        let credentials = RtcCredentials::new();

        let (session, room_events) =
            match Room::connect(&credentials.url, &credentials.token, RoomOptions::default()).await
            {
                Ok((room, room_events)) => (Arc::new(RemoteAccessSession::new(room)), room_events),
                Err(e) => {
                    return Err(e.into());
                }
            };

        Ok((session, room_events))
    }

    /// Returns the cancellation token for the [`RemoteAccessConnection`]`.
    fn cancellation_token(&self) -> &CancellationToken {
        &self.options.cancellation_token
    }

    /// Run the server loop until cancelled in a new tokio task.
    ///
    /// If disconnected from the room, reset all state and attempt to restart the run loop.
    pub fn spawn_run_until_cancelled(self: Arc<Self>) -> JoinHandle<()> {
        if let Some(runtime) = self.options.runtime.as_ref() {
            runtime.spawn(self.clone().run_until_cancelled())
        } else {
            tokio::spawn(self.run_until_cancelled())
        }
    }

    /// Run the server loop until cancelled.
    ///
    /// If disconnected from the room, reset all state and attempt to restart the run loop.
    async fn run_until_cancelled(self: Arc<Self>) {
        while !self.cancellation_token().is_cancelled() {
            self.run().await;
        }
    }

    /// Connect to the room, and handle all events until cancelled or disconnected from the room.
    async fn run(&self) {
        let Some((session, room_events)) = self.connect_session_until_ok().await else {
            // Cancelled/shutting down
            debug_assert!(self.cancellation_token().is_cancelled());
            return;
        };

        let attributes = session.room.local_participant().attributes();
        let identity = session.room.local_participant().identity();
        info!(
            "local participant {:?} attributes: {:?}",
            identity, attributes
        );

        info!("running remote access server");
        tokio::select! {
            () = self.cancellation_token().cancelled() => (),
            _ = self.listen_for_room_events(session.clone(), room_events) => {}
        }

        info!("disconnecting from room");
        // Close the room (disconnect) on shutdown.
        // If we don't do that, there's a 15s delay before this device is removed from the participants
        if let Err(e) = session.room.close().await {
            error!("failed to close room: {e:?}");
        }
    }

    /// Connect to the room, retrying indefinitely.
    ///
    /// Only returns an error if the connection has been permanently stopped/cancelled (shutting down).
    ///
    /// The retry interval is fairly long.
    /// Note that livekit internally includes a few quick retries for each connect call as well.
    async fn connect_session_until_ok(
        &self,
    ) -> Option<(Arc<RemoteAccessSession>, UnboundedReceiver<RoomEvent>)> {
        let mut interval = tokio::time::interval(AUTH_RETRY_PERIOD);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                () = self.cancellation_token().cancelled() => {
                    return None;
                }
            };

            let result = tokio::select! {
                () = self.cancellation_token().cancelled() => {
                    return None;
                }
                result = self.connect_session() => result,
            };

            match result {
                Ok((session, room_events)) => {
                    return Some((session, room_events));
                }
                Err(RemoteAccessError::ConnectionError(e)) => {
                    error!("{e:?}");

                    // We can't inspect the inner types of Engine errors; this may be caused by
                    // general connectivity issues, or be auth-related. Attempt to refresh the
                    // credentials in case they've expired.
                    // TODO refresh credentials
                }
                Err(e) => {
                    error!(
                        "failed to establish remote access connection: {e:?}, retrying in {AUTH_RETRY_PERIOD:?}"
                    );
                }
            }
        }
    }

    async fn listen_for_room_events(
        &self,
        session: Arc<RemoteAccessSession>,
        mut room_events: UnboundedReceiver<RoomEvent>,
    ) {
        while let Some(event) = room_events.recv().await {
            debug!("room event: {:?}", event);
            match event {
                RoomEvent::ParticipantConnected(participant) => {
                    info!("entered the room: {:?}", participant.identity());
                    let participant_id = match session.add_participant(participant.identity()).await
                    {
                        Ok(id) => id,
                        Err(e) => {
                            error!("failed to add participant: {e:?}");
                            continue;
                        }
                    };

                    let server_info = self.create_server_info();
                    session
                        .send_info_and_advertisements(participant_id, server_info)
                        .await;
                }
                RoomEvent::ParticipantDisconnected(participant) => {
                    let mut participants = session.participants.write();
                    let participant_id = participant.identity();
                    if participants.remove(&participant_id).is_some() {
                        info!("removed participant {participant_id:?}");
                    }
                }
                RoomEvent::DataReceived {
                    payload: _,
                    topic,
                    kind: _,
                    participant: _,
                } => {
                    info!("data received: {:?}", topic);
                }
                RoomEvent::ByteStreamOpened {
                    reader: _,
                    topic: _,
                    participant_identity,
                } => {
                    info!(
                        "byte stream opened from participant: {:?}",
                        participant_identity
                    );
                    // TODO handle byte stream from client
                }
                RoomEvent::Disconnected { reason } => {
                    info!(
                        "disconnected: {:?}, will attempt to reconnect",
                        reason.as_str_name()
                    );
                    // Return from this function to trigger reconnection in run_until_cancelled
                    return;
                }
                _ => {}
            }
        }
        warn!("stopped listening for room events");
    }

    /// Create and serialize ServerInfo message based on the RemoteAccessConnectionOptions.
    ///
    /// The metadata and supported_encodings are important for the ClientPublish capability,
    /// as some app components will use this information to determine publish formats (ROS1 vs. JSON).
    /// For example, a ros-foxglove-bridge source may advertise the "ros1" supported encoding
    /// and "ROS_DISTRO": "melodic" metadata.
    ///
    /// We always add our own fg-library metadata.
    pub fn create_server_info(&self) -> ServerInfo {
        let mut metadata = self.options.server_info.clone().unwrap_or_default();
        let supported_encodings = self.options.supported_encodings.clone();
        metadata.insert("fg-library".into(), get_library_version());

        let mut info = ServerInfo::new("remote_access")
            .with_session_id(self.options.session_id.clone())
            .with_capabilities(
                self.options
                    .capabilities
                    .iter()
                    .flat_map(|c| c.as_protocol_capabilities())
                    .copied(),
            )
            .with_metadata(metadata);

        if let Some(supported_encodings) = supported_encodings {
            info = info.with_supported_encodings(supported_encodings);
        }

        info
    }

    pub(crate) fn shutdown(&self) {
        self.cancellation_token().cancel();
    }
}

impl RemoteAccessSession {
    fn new(room: Room) -> Self {
        Self {
            room,
            participants: RwLock::new(HashMap::new()),
        }
    }

    /// Add a participant to the server, if it hasn't already been added.
    /// In either case, return the new or existing participant.
    async fn add_participant(
        &self,
        participant_id: ParticipantIdentity,
    ) -> Result<Arc<Participant>> {
        // First, check if we already have this participant by identity
        {
            if let Some(existing_participant) = self.participants.read().get(&participant_id) {
                return Ok(existing_participant.clone());
            }
        }

        let stream = match self
            .room
            .local_participant()
            .stream_bytes(StreamByteOptions {
                topic: WS_PROTOCOL_TOPIC.to_string(),
                destination_identities: vec![participant_id.clone()],
                ..StreamByteOptions::default()
            })
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                error!("failed to create stream for participant {participant_id}: {e:?}");
                return Err(e.into());
            }
        };

        let participant = Arc::new(Participant::new(
            participant_id.clone(),
            ParticipantWriter::Livekit(stream),
        ));

        self.participants
            .write()
            .insert(participant_id, participant.clone());
        Ok(participant)
    }

    async fn send_info_and_advertisements(
        &self,
        participant: Arc<Participant>,
        server_info: ServerInfo,
    ) {
        info!("sending server info and advertisements to participant {participant:?}");
        self.send_to_participant(participant, server_info.to_string().into(), OpCode::Text)
            .await;
        // TODO send advertisements
        //self.send_advertisements(participant_id).await;
    }

    // Send a message to one participant identified by the id
    async fn send_to_participant(
        &self,
        participant: Arc<Participant>,
        bytes: Bytes,
        op_code: OpCode,
    ) {
        // Add the message framing, 1 byte op code + 4 byte little-endian length
        let mut buf = SmallVec::<[u8; 4 * 1024]>::with_capacity(bytes.len() + 5);
        buf.push(op_code as u8);
        buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&bytes);

        if let Err(e) = participant.send(&buf).await {
            error!("failed to send to participant {participant:?}: {e:?}");
        }
    }
}

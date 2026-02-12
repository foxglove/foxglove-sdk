use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use arc_swap::ArcSwapOption;
use bytes::Bytes;
use livekit::{id::ParticipantIdentity, Room, RoomEvent, RoomOptions, StreamByteOptions};
use parking_lot::RwLock;
use smallvec::SmallVec;
use tokio::{runtime::Handle, sync::mpsc::UnboundedReceiver};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    cloud::{
        participant::{Participant, ParticipantWriter},
        CloudError,
    },
    library_version::get_library_version,
    websocket::{self, Server},
    ws_protocol::{server::ServerInfo, JsonMessage},
    CloudSinkListener, SinkChannelFilter,
};

type Result<T> = std::result::Result<T, CloudError>;

const WS_PROTOCOL_TOPIC: &str = "ws-protocol";
#[allow(dead_code)]
const MESSAGE_FRAME_SIZE: usize = 5;
const AUTH_RETRY_PERIOD: Duration = Duration::from_secs(30);

/// The operation code for the message framing for protocol v2.
/// Distinguishes between frames containing JSON messages vs binary messages.
#[derive(Clone, Copy, Debug)]
#[repr(u8)]
enum OpCode {
    /// The frame contains a JSON message.
    Text = 1,
    /// The frame contains a binary message.
    #[allow(dead_code)]
    Binary = 2,
}

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

/// Options for the cloud connection.
///
/// This should be constructed from the [`crate::CloudSink`] builder.
#[derive(Clone)]
pub(crate) struct CloudConnectionOptions {
    pub session_id: String,
    pub listener: Option<Arc<dyn CloudSinkListener>>,
    pub capabilities: Vec<websocket::Capability>,
    pub supported_encodings: Option<HashSet<String>>,
    pub runtime: Option<Handle>,
    pub channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    pub server_info: Option<HashMap<String, String>>,
    pub cancellation_token: CancellationToken,
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
            cancellation_token: CancellationToken::new(),
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

/// CloudSession tracks a connected LiveKit session (the Room)
/// and any state that is specific to that session.
/// We discard this state if we close or lose the connection.
struct CloudSession {
    room: Room,
    participants: RwLock<HashMap<ParticipantIdentity, Arc<Participant>>>,
}

/// CloudConnection manages the connected session to the LiveKit server,
/// and holds the options and other state that outlive a session.
pub(crate) struct CloudConnection {
    options: CloudConnectionOptions,
    /// The current session, if any.
    session: ArcSwapOption<CloudSession>,
}

impl CloudConnection {
    pub fn new(options: CloudConnectionOptions) -> Self {
        Self {
            options,
            session: ArcSwapOption::new(None),
        }
    }

    async fn connect_session(&self) -> Result<(Arc<CloudSession>, UnboundedReceiver<RoomEvent>)> {
        // TODO get credentials from API
        let credentials = RtcCredentials::new();

        let (session, room_events) =
            match Room::connect(&credentials.url, &credentials.token, RoomOptions::default()).await
            {
                Ok((room, room_events)) => (Arc::new(CloudSession::new(room)), room_events),
                Err(e) => {
                    return Err(e.into());
                }
            };
        self.session.store(Some(session.clone()));

        Ok((session, room_events))
    }

    /// Returns the cancellation token for the [`CloudConnection`]`.
    fn cancellation_token(&self) -> &CancellationToken {
        &self.options.cancellation_token
    }

    /// Run the server loop until cancelled.
    ///
    /// If disconnected from the room, reset all state and attempt to restart the run loop.
    pub async fn run_until_cancelled(self: Arc<Self>) {
        while !self.cancellation_token().is_cancelled() {
            self.run().await;
        }
    }

    /// Connect to the room, and handle all events until cancelled or disconnected from the room.
    async fn run(&self) {
        let Ok((session, room_events)) = self.connect_session_until_ok().await else {
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

        info!("running cloud server");
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
    ) -> Result<(Arc<CloudSession>, UnboundedReceiver<RoomEvent>)> {
        let mut interval = tokio::time::interval(AUTH_RETRY_PERIOD);
        loop {
            interval.tick().await;

            let result = tokio::select! {
                () = self.cancellation_token().cancelled() => {
                    return Err(CloudError::ConnectionStopped);
                }
                result = self.connect_session() => result,
            };

            match result {
                Ok((session, room_events)) => {
                    return Ok((session, room_events));
                }
                Err(CloudError::ConnectionStopped) => {
                    return Err(CloudError::ConnectionStopped);
                }
                Err(CloudError::ConnectionError(e)) => {
                    error!("{e:?}");

                    // We can't inspect the inner types of Engine errors; this may be caused by
                    // general connectivity issues, or be auth-related. Attempt to refresh the
                    // credentials in case they've expired.
                    // TODO refresh credentials
                }
                Err(e) => {
                    error!(
                        "failed to establish cloud connection: {e:?}, retrying in {AUTH_RETRY_PERIOD:?}"
                    );
                }
            }
        }
    }

    async fn listen_for_room_events(
        &self,
        session: Arc<CloudSession>,
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
                            return;
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
                    if let Some(_) = participants.remove(&participant_id) {
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
                    participant_identity: _,
                } => {
                    // if let Some(reader) = reader.take() {
                    //     let session2 = session.clone();
                    //     tokio::spawn(async move {
                    //         session2
                    //             .handle_byte_stream_from_client(participant_identity, reader)
                    //             .await;
                    //     });
                    // }
                }
                RoomEvent::Disconnected { reason } => {
                    info!("disconnected: {:?}", reason.as_str_name());
                }
                _ => {}
            }
        }
        warn!("stopped listening for room events");
    }

    /// Create and serialize ServerInfo message based on the CloudConnectionOptions.
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

        let mut info = ServerInfo::new("cloud")
            .with_session_id(self.options.session_id.clone())
            .with_capabilities(
                self.options
                    .capabilities
                    .iter()
                    .map(|c| c.as_protocol_capabilities())
                    .flatten()
                    .cloned(),
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

impl CloudSession {
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

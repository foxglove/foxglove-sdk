use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use bytes::{Bytes, BytesMut};
use futures::StreamExt;
use livekit::{
    id::ParticipantIdentity, ByteStreamReader, Room, RoomEvent, RoomOptions, StreamByteOptions,
};
use parking_lot::RwLock;
use smallvec::SmallVec;
use tokio::{runtime::Handle, sync::mpsc::UnboundedReceiver};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    library_version::get_library_version,
    protocol::v2::{
        client::{self, ClientMessage},
        server::{MessageData as ServerMessageData, ServerInfo, Unadvertise},
        BinaryMessage, JsonMessage,
    },
    remote_access::{
        participant::{Participant, ParticipantWriter},
        RemoteAccessError,
    },
    websocket::{self, advertise, Server},
    ChannelId, Context, FoxgloveError, Metadata, RawChannel, RemoteAccessSinkListener, Sink,
    SinkChannelFilter, SinkId,
};

type Result<T> = std::result::Result<T, RemoteAccessError>;

const WS_PROTOCOL_TOPIC: &str = "ws-protocol";
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
            .finish()
    }
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
                Ok((room, room_events)) => (
                    Arc::new(RemoteAccessSession::new(
                        room,
                        self.options.channel_filter.clone(),
                    )),
                    room_events,
                ),
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
        let Some((session, room_events)) = self.connect_session_until_ok().await else {
            // Cancelled/shutting down
            debug_assert!(self.cancellation_token().is_cancelled());
            return;
        };

        // Register the session as a sink so it receives channel notifications.
        // This synchronously triggers add_channels for all existing channels.
        self.options.context.add_sink(session.clone());

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

        // Remove the sink before closing the room.
        self.options.context.remove_sink(session.sink_id);

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
                        .send_info_and_advertisements(&participant_id, server_info)
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
                    reader,
                    topic: _,
                    participant_identity,
                } => {
                    // This is how we handle incoming reliable messages from the client
                    // They open a byte stream to the device participant (us).
                    info!(
                        "byte stream opened from participant: {:?}",
                        participant_identity
                    );
                    if let Some(reader) = reader.take() {
                        let session2 = session.clone();
                        tokio::spawn(async move {
                            session2
                                .handle_byte_stream_from_client(participant_identity, reader)
                                .await;
                        });
                    }
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

/// Frames a payload with the v2 message framing (1 byte opcode + 4 byte LE length + payload).
fn frame_message(payload: &[u8], op_code: OpCode) -> Vec<u8> {
    let mut buf = Vec::with_capacity(payload.len() + MESSAGE_FRAME_SIZE);
    buf.push(op_code as u8);
    let len = u32::try_from(payload.len()).expect("message too large");
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(payload);
    buf
}

/// RemoteAccessSession tracks a connected LiveKit session (the Room)
/// and any state that is specific to that session.
/// We discard this state if we close or lose the connection.
/// [`RemoteAccessConnection`] manages the current connected session (if any)
///
/// The Sink impl is at the RemoteAccessSession level (not per-participant)
/// so that it can deliver messages via multi-cast to multiple participants.
struct RemoteAccessSession {
    sink_id: SinkId,
    room: Room,
    participants: RwLock<HashMap<ParticipantIdentity, Arc<Participant>>>,
    /// Channels that have been advertised to participants.
    channels: RwLock<HashMap<ChannelId, Arc<RawChannel>>>,
    channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    /// Handle to the tokio runtime, used to spawn async sends from sync Sink callbacks.
    runtime: Handle,
}

impl Sink for RemoteAccessSession {
    fn id(&self) -> SinkId {
        self.sink_id
    }

    fn log(
        &self,
        channel: &RawChannel,
        msg: &[u8],
        metadata: &Metadata,
    ) -> std::result::Result<(), FoxgloveError> {
        let channel_id = channel.id();

        // Collect the subscribed participants
        // Use a SmallVec to avoid alloc+free on each message logged
        let participants: SmallVec<[Arc<Participant>; 8]> = {
            let participants = self.participants.read();
            participants
                .values()
                .filter(|p| p.is_subscribed(channel_id))
                .cloned()
                .collect()
        };

        if participants.is_empty() {
            return Ok(());
        }

        let message = ServerMessageData::new(u32::from(channel_id), metadata.log_time, msg);
        let framed = frame_message(&message.to_bytes(), OpCode::Binary);

        self.runtime.spawn(async move {
            for participant in participants {
                if let Err(e) = participant.send(&framed).await {
                    error!("failed to send message data to {participant:?}: {e:?}");
                }
            }
        });

        Ok(())
    }

    fn add_channels(&self, channels: &[&Arc<RawChannel>]) -> Option<Vec<ChannelId>> {
        let filtered: Vec<_> = channels
            .iter()
            .filter(|ch| {
                let Some(filter) = self.channel_filter.as_ref() else {
                    return true;
                };
                filter.should_subscribe(ch.descriptor())
            })
            .copied()
            .collect();

        if filtered.is_empty() {
            return None;
        }

        let advertise_msg = advertise::advertise_channels(filtered.iter().copied());

        // Cache channels
        {
            let mut cached = self.channels.write();
            for &ch in &filtered {
                cached.insert(ch.id(), ch.clone());
            }
        }

        if advertise_msg.channels.is_empty() {
            return None;
        }

        let framed = frame_message(advertise_msg.to_string().as_bytes(), OpCode::Text);

        let participants: Vec<Arc<Participant>> =
            self.participants.read().values().cloned().collect();
        if !participants.is_empty() {
            self.runtime.spawn(async move {
                for participant in participants {
                    if let Err(e) = participant.send(&framed).await {
                        error!("failed to send channel advertisement to {participant:?}: {e:?}");
                    }
                }
            });
        }

        None
    }

    fn remove_channel(&self, channel: &RawChannel) {
        let channel_id = channel.id();
        if self.channels.write().remove(&channel_id).is_none() {
            return;
        }

        let unadvertise = Unadvertise::new([u64::from(channel_id)]);
        let framed = frame_message(unadvertise.to_string().as_bytes(), OpCode::Text);

        let participants: Vec<Arc<Participant>> =
            self.participants.read().values().cloned().collect();
        if !participants.is_empty() {
            self.runtime.spawn(async move {
                for participant in participants {
                    if let Err(e) = participant.send(&framed).await {
                        error!("failed to send channel unadvertisement to {participant:?}: {e:?}");
                    }
                }
            });
        }
    }

    fn auto_subscribe(&self) -> bool {
        false
    }
}

impl RemoteAccessSession {
    fn new(room: Room, channel_filter: Option<Arc<dyn SinkChannelFilter>>) -> Self {
        Self {
            sink_id: SinkId::next(),
            room,
            participants: RwLock::new(HashMap::new()),
            channels: RwLock::new(HashMap::new()),
            channel_filter,
            runtime: Handle::current(),
        }
    }

    async fn handle_byte_stream_from_client(
        self: &Arc<Self>,
        participant_identity: ParticipantIdentity,
        mut reader: ByteStreamReader,
    ) {
        let mut buffer = BytesMut::new();
        loop {
            // Read chunk from stream
            let chunk = match reader.next().await {
                Some(Ok(chunk)) => chunk,
                Some(Err(e)) => {
                    error!(
                        "Error reading from byte stream for client {:?}: {:?}",
                        participant_identity, e
                    );
                    break;
                }
                None => {
                    break;
                }
            };

            buffer.extend_from_slice(&chunk);

            // Parse complete messages from buffer
            while buffer.len() >= MESSAGE_FRAME_SIZE {
                // Parse frame header: 1 byte OpCode + 4 bytes little-endian u32 length
                let opcode = buffer[0];
                let length =
                    u32::from_le_bytes(buffer[1..MESSAGE_FRAME_SIZE].try_into().unwrap()) as usize;

                // Check if we have the complete message
                if buffer.len() < MESSAGE_FRAME_SIZE + length {
                    break; // Wait for more data
                }

                // Split off the header (opcode + length) and payload as a single message
                let message = buffer.split_to(MESSAGE_FRAME_SIZE + length);

                // Extract the payload without copying by splitting off the header
                let payload = message.freeze().slice(MESSAGE_FRAME_SIZE..);

                // Create a simple message structure with OpCode and payload
                // Since we don't know the exact structure of ClientMessage,
                // we'll pass the raw data to handle_client_message
                self.handle_client_message(&participant_identity, opcode, payload);
            }
        }
    }

    fn handle_client_message(
        self: &Arc<Self>,
        participant_identity: &ParticipantIdentity,
        opcode: u8,
        payload: Bytes,
    ) {
        const TEXT: u8 = OpCode::Text as u8;
        const BINARY: u8 = OpCode::Binary as u8;
        let client_msg = match opcode {
            TEXT => match std::str::from_utf8(&payload) {
                Ok(text) => ClientMessage::parse_json(text),
                Err(e) => {
                    error!("Invalid UTF-8 in text message: {e:?}");
                    return;
                }
            },
            BINARY => ClientMessage::parse_binary(&payload[..]),
            _ => {
                error!("Invalid opcode: {opcode}");
                return;
            }
        };

        let client_msg = match client_msg {
            Ok(msg) => msg,
            Err(e) => {
                error!("failed to parse client message: {e:?}");
                return;
            }
        };

        // Look up participant ID before the final match
        let Some(participant) = ({
            let participants = self.participants.read();
            participants.get(participant_identity).cloned()
        }) else {
            error!("Unknown participant identity: {:?}", participant_identity);
            return;
        };

        match client_msg {
            ClientMessage::Subscribe(msg) => {
                self.handle_client_subscribe(&participant, msg);
            }
            ClientMessage::Unsubscribe(msg) => {
                self.handle_client_unsubscribe(&participant, msg);
            }
            // TODO: Implement other message handling branches
            _ => {}
        }
    }

    fn handle_client_subscribe(&self, participant: &Participant, msg: client::Subscribe) {
        let channel_ids: Vec<ChannelId> = msg
            .channel_ids
            .iter()
            .map(|&id| ChannelId::new(id))
            .collect();

        let newly_subscribed = participant.subscribe(&channel_ids);

        for &channel_id in &channel_ids {
            if newly_subscribed.contains(&channel_id) {
                debug!(
                    "Participant {:?} subscribed to channel {channel_id:?}",
                    participant
                );
            } else {
                warn!(
                    "Participant {:?} is already subscribed to channel {channel_id:?}; ignoring",
                    participant
                );
            }
        }
    }

    fn handle_client_unsubscribe(&self, participant: &Participant, msg: client::Unsubscribe) {
        let channel_ids: Vec<ChannelId> = msg
            .channel_ids
            .iter()
            .map(|&id| ChannelId::new(id))
            .collect();

        let unsubscribed = participant.unsubscribe(&channel_ids);

        for &channel_id in &channel_ids {
            if unsubscribed.contains(&channel_id) {
                debug!(
                    "Participant {:?} unsubscribed from channel {channel_id:?}",
                    participant
                );
            } else {
                warn!(
                    "Participant {:?} is not subscribed to channel {channel_id:?}; ignoring",
                    participant
                );
            }
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
        participant: &Participant,
        server_info: ServerInfo,
    ) {
        info!("sending server info and advertisements to participant {participant:?}");
        Self::send_to_participant(
            participant,
            server_info.to_string().as_bytes(),
            OpCode::Text,
        )
        .await;
        self.send_channel_advertisements(participant).await;
    }

    /// Send all currently cached channel advertisements to a single participant.
    async fn send_channel_advertisements(&self, participant: &Participant) {
        let advertise_bytes = {
            let channels = self.channels.read();
            if channels.is_empty() {
                return;
            }
            let advertise_msg = advertise::advertise_channels(channels.values());
            if advertise_msg.channels.is_empty() {
                return;
            }
            advertise_msg.to_string()
        };

        Self::send_to_participant(participant, advertise_bytes.as_bytes(), OpCode::Text).await;
    }

    /// Send a framed message to one participant.
    async fn send_to_participant(participant: &Participant, payload: &[u8], op_code: OpCode) {
        let framed = frame_message(payload, op_code);
        if let Err(e) = participant.send(&framed).await {
            error!("failed to send to participant {participant:?}: {e:?}");
        }
    }
}

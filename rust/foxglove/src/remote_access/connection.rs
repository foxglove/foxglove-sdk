use std::{
    collections::HashMap,
    sync::{Arc, Weak},
    time::Duration,
};

use indexmap::IndexSet;

use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use livekit::{
    id::ParticipantIdentity, ByteStreamReader, Room, RoomEvent, RoomOptions, StreamByteOptions,
};
use parking_lot::RwLock;
use smallvec::SmallVec;
use tokio::{runtime::Handle, sync::mpsc::UnboundedReceiver, sync::OnceCell, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    api_client::{DeviceToken, FoxgloveApiClientBuilder},
    library_version::get_library_version,
    protocol::v2::{
        client::{self, ClientMessage},
        server::{advertise, MessageData as ServerMessageData, ServerInfo, Unadvertise},
        BinaryMessage, JsonMessage,
    },
    remote_access::{
        credentials_provider::CredentialsProvider,
        participant::{Participant, ParticipantWriter},
        RemoteAccessError,
    },
    websocket::{self, Server},
    ChannelId, Context, FoxgloveError, Metadata, RawChannel, RemoteAccessSinkListener, Sink,
    SinkChannelFilter, SinkId,
};

type Result<T> = std::result::Result<T, RemoteAccessError>;

const WS_PROTOCOL_TOPIC: &str = "ws-protocol";
const MESSAGE_FRAME_SIZE: usize = 5; // 1 byte opcode + u32 LE length
const AUTH_RETRY_PERIOD: Duration = Duration::from_secs(30);
const DEFAULT_MESSAGE_BACKLOG_SIZE: usize = 1024;
const MAX_SEND_RETRIES: usize = 3;
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16 MiB

/// A data plane message queued for delivery to subscribed participants.
struct ChannelMessage {
    channel_id: ChannelId,
    data: Bytes,
}

/// A control plane message queued for delivery to a specific participant.
struct ControlPlaneMessage {
    participant: Arc<Participant>,
    data: Bytes,
}

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

/// Options for the remote access connection.
///
/// This should be constructed from the [`crate::RemoteAccessSink`] builder.
#[derive(Clone)]
pub(crate) struct RemoteAccessConnectionOptions {
    pub name: Option<String>,
    pub device_token: String,
    pub foxglove_api_url: Option<String>,
    pub foxglove_api_timeout: Option<Duration>,
    pub session_id: String,
    pub listener: Option<Arc<dyn RemoteAccessSinkListener>>,
    pub capabilities: Vec<websocket::Capability>,
    pub supported_encodings: Option<IndexSet<String>>,
    pub runtime: Option<Handle>,
    pub channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    pub server_info: Option<HashMap<String, String>>,
    pub message_backlog_size: Option<usize>,
    pub cancellation_token: CancellationToken,
    pub context: Weak<Context>,
}

impl Default for RemoteAccessConnectionOptions {
    fn default() -> Self {
        Self {
            name: None,
            device_token: String::new(),
            foxglove_api_url: None,
            foxglove_api_timeout: None,
            session_id: Server::generate_session_id(),
            listener: None,
            capabilities: Vec::new(),
            supported_encodings: None,
            runtime: None,
            channel_filter: None,
            server_info: None,
            message_backlog_size: None,
            cancellation_token: CancellationToken::new(),
            context: Arc::downgrade(&Context::get_default()),
        }
    }
}

impl std::fmt::Debug for RemoteAccessConnectionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteAccessConnectionOptions")
            .field("name", &self.name)
            .field("has_device_token", &!self.device_token.is_empty())
            .field("foxglove_api_url", &self.foxglove_api_url)
            .field("foxglove_api_timeout", &self.foxglove_api_timeout)
            .field("session_id", &self.session_id)
            .field("has_listener", &self.listener.is_some())
            .field("capabilities", &self.capabilities)
            .field("supported_encodings", &self.supported_encodings)
            .field("has_runtime", &self.runtime.is_some())
            .field("has_channel_filter", &self.channel_filter.is_some())
            .field("server_info", &self.server_info)
            .field("message_backlog_size", &self.message_backlog_size)
            .field("has_context", &(self.context.strong_count() > 0))
            .finish()
    }
}

/// RemoteAccessConnection manages the connected [`RemoteAccessSession`] to the LiveKit server,
/// and holds the options and other state that outlive a session.
pub(crate) struct RemoteAccessConnection {
    options: RemoteAccessConnectionOptions,
    credentials_provider: OnceCell<CredentialsProvider>,
}

impl RemoteAccessConnection {
    pub fn new(options: RemoteAccessConnectionOptions) -> Self {
        Self {
            options,
            credentials_provider: OnceCell::new(),
        }
    }

    /// Returns the credentials provider, initializing it on first call.
    ///
    /// This fetches device info from the Foxglove API using the device token.
    /// If the call fails, the OnceCell remains empty and will retry on the next call.
    async fn get_or_init_provider(&self) -> Result<&CredentialsProvider> {
        self.credentials_provider
            .get_or_try_init(|| async {
                let mut builder = FoxgloveApiClientBuilder::new(DeviceToken::new(
                    self.options.device_token.clone(),
                ));
                if let Some(url) = &self.options.foxglove_api_url {
                    builder = builder.base_url(url);
                }
                if let Some(timeout) = self.options.foxglove_api_timeout {
                    builder = builder.timeout(timeout);
                }
                CredentialsProvider::new(builder)
                    .await
                    .map_err(|e| RemoteAccessError::AuthError(e.to_string()))
            })
            .await
    }

    async fn connect_session(
        &self,
    ) -> Result<(Arc<RemoteAccessSession>, UnboundedReceiver<RoomEvent>)> {
        let provider = self.get_or_init_provider().await?;
        let credentials = provider
            .load_credentials()
            .await
            .map_err(|e| RemoteAccessError::AuthError(e.to_string()))?;

        let message_backlog_size = self
            .options
            .message_backlog_size
            .unwrap_or(DEFAULT_MESSAGE_BACKLOG_SIZE);

        let (session, room_events) =
            match Room::connect(&credentials.url, &credentials.token, RoomOptions::default()).await
            {
                Ok((room, room_events)) => (
                    Arc::new(RemoteAccessSession::new(
                        room,
                        self.options.context.clone(),
                        self.options.channel_filter.clone(),
                        self.options.cancellation_token.clone(),
                        message_backlog_size,
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

        // Register the session as a sink so it receives channel notifications.
        // This synchronously triggers add_channels for all existing channels.
        let Some(context) = self.options.context.upgrade() else {
            info!("context has been dropped, stopping remote access connection");
            return;
        };
        context.add_sink(session.clone());

        // We can use spawn here because we're already running on self.options.runtime (if set)
        let sender_task = tokio::spawn(RemoteAccessSession::run_sender(session.clone()));

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
        context.remove_sink(session.sink_id);
        sender_task.abort();

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
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
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
                Err(e) => {
                    error!("{e}");
                    // Refresh credentials if we experience an AuthError. We also do this for
                    // RoomErrors, which may be auth-related, and for which we do not have any
                    // distinguishing type information at this point.
                    if matches!(
                        e,
                        RemoteAccessError::AuthError(_) | RemoteAccessError::RoomError(_)
                    ) {
                        if let Some(provider) = self.credentials_provider.get() {
                            debug!("clearing credentials");
                            provider.clear().await;
                        }
                    }
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
                    session.send_info_and_advertisements(participant_id.clone(), server_info);
                }
                RoomEvent::ParticipantDisconnected(participant) => {
                    session.remove_participant(&participant.identity());
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

        // The credentials provider is always initialized before this method is called,
        // since we must successfully connect (which initializes the provider) before we
        // can receive room events that trigger server info creation.
        let name = self.options.name.clone().unwrap_or_else(|| {
            self.credentials_provider
                .get()
                .map(|p| p.device_name().to_string())
                .unwrap_or_default()
        });

        let mut info = ServerInfo::new(name)
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

/// Frames a text payload with the v2 message framing (1 byte opcode + 4 byte LE length + payload).
fn frame_text_message(payload: &[u8]) -> Bytes {
    let mut buf = Vec::with_capacity(MESSAGE_FRAME_SIZE + payload.len());
    buf.push(OpCode::Text as u8);
    let len = u32::try_from(payload.len()).expect("message too large");
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(payload);
    Bytes::from(buf)
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
    context: Weak<Context>,
    participants: RwLock<HashMap<ParticipantIdentity, Arc<Participant>>>,
    /// Channels that have been advertised to participants.
    channels: RwLock<HashMap<ChannelId, Arc<RawChannel>>>,
    /// Maps channel ID to the participant identities subscribed to that channel.
    subscriptions: RwLock<HashMap<ChannelId, SmallVec<[ParticipantIdentity; 1]>>>,
    channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    cancellation_token: CancellationToken,
    data_plane_tx: flume::Sender<ChannelMessage>,
    data_plane_rx: flume::Receiver<ChannelMessage>,
    control_plane_tx: flume::Sender<ControlPlaneMessage>,
    control_plane_rx: flume::Receiver<ControlPlaneMessage>,
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
        let message = ServerMessageData::new(u32::from(channel_id), metadata.log_time, msg);
        let data = encode_binary_message(&message);
        self.send_data_lossy(ChannelMessage { channel_id, data });
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

        // Track advertised channels
        {
            let mut cached = self.channels.write();
            for &ch in &filtered {
                cached.insert(ch.id(), ch.clone());
            }
        }

        if advertise_msg.channels.is_empty() {
            return None;
        }

        let framed = frame_text_message(advertise_msg.to_string().as_bytes());
        self.broadcast_control(framed);

        // Clients subscribe asynchronously.
        None
    }

    fn remove_channel(&self, channel: &RawChannel) {
        let channel_id = channel.id();
        if self.channels.write().remove(&channel_id).is_none() {
            return;
        }

        let unadvertise = Unadvertise::new([u64::from(channel_id)]);
        let framed = frame_text_message(unadvertise.to_string().as_bytes());
        self.broadcast_control(framed);
    }

    fn auto_subscribe(&self) -> bool {
        false
    }
}

fn encode_binary_message<'a>(message: &impl BinaryMessage<'a>) -> Bytes {
    let msg_len = message.encoded_len();
    let mut buf = Vec::with_capacity(MESSAGE_FRAME_SIZE + msg_len);
    buf.push(OpCode::Binary as u8);
    buf.extend_from_slice(
        &u32::try_from(msg_len)
            .expect("message too large")
            .to_le_bytes(),
    );
    message.encode(&mut buf);
    Bytes::from(buf)
}

impl RemoteAccessSession {
    fn new(
        room: Room,
        context: Weak<Context>,
        channel_filter: Option<Arc<dyn SinkChannelFilter>>,
        cancellation_token: CancellationToken,
        message_backlog_size: usize,
    ) -> Self {
        let (data_plane_tx, data_plane_rx) = flume::bounded(message_backlog_size);
        let (control_plane_tx, control_plane_rx) = flume::bounded(message_backlog_size);
        Self {
            sink_id: SinkId::next(),
            room,
            context,
            participants: RwLock::new(HashMap::new()),
            channels: RwLock::new(HashMap::new()),
            subscriptions: RwLock::new(HashMap::new()),
            channel_filter,
            cancellation_token,
            data_plane_tx,
            data_plane_rx,
            control_plane_tx,
            control_plane_rx,
        }
    }

    /// Enqueue a data plane message, dropping old messages if the queue is full.
    fn send_data_lossy(&self, mut msg: ChannelMessage) {
        static THROTTLER: parking_lot::Mutex<crate::throttler::Throttler> =
            parking_lot::Mutex::new(crate::throttler::Throttler::new(Duration::from_secs(30)));
        let mut dropped = 0;
        loop {
            match self.data_plane_tx.try_send(msg) {
                Ok(_) => {
                    if dropped > 0 && THROTTLER.lock().try_acquire() {
                        info!("data plane queue full, dropped {dropped} message(s)");
                    }
                    return;
                }
                Err(flume::TrySendError::Disconnected(_)) => return,
                Err(flume::TrySendError::Full(rejected)) => {
                    if dropped >= MAX_SEND_RETRIES {
                        if THROTTLER.lock().try_acquire() {
                            info!("data plane queue full, dropped message");
                        }
                        return;
                    }
                    msg = rejected;
                    let _ = self.data_plane_rx.try_recv();
                    dropped += 1;
                }
            }
        }
    }

    /// Enqueue a control plane message for a specific participant.
    /// Blocks the thread if the queue is full.
    fn send_control(&self, participant: Arc<Participant>, data: Bytes) {
        let msg = ControlPlaneMessage { participant, data };
        if let Err(e) = self.control_plane_tx.send(msg) {
            warn!("control plane queue disconnected, dropping message: {e}");
        }
    }

    /// Enqueue a control plane message for all currently connected participants.
    fn broadcast_control(&self, data: Bytes) {
        let participants = self.participants.read();
        for participant in participants.values() {
            self.send_control(participant.clone(), data.clone());
        }
    }

    /// Reads from the data plane and control plane queues and sends messages to participants.
    ///
    /// Control plane messages are sent to the targeted participant.
    /// Data plane messages are sent to all participants subscribed to the message's channel.
    async fn run_sender(session: Arc<Self>) {
        loop {
            tokio::select! {
                biased;
                () = session.cancellation_token.cancelled() => break,
                msg = session.control_plane_rx.recv_async() => {
                    let Ok(msg) = msg else { break };
                    if let Err(e) = msg.participant.send(&msg.data).await {
                        error!("failed to send control message to {:?}: {e:?}", msg.participant);
                    }
                }
                msg = session.data_plane_rx.recv_async() => {
                    let Ok(msg) = msg else { break };
                    // Note: we do fan-out ourselves here because we can't use multicast with the ByteStreams
                    // Most data plane messages should get sent as datagram messages, which do support multicast
                    // by passing a Vec<ParticipantIdentity> of recipients.
                    let subscriber_ids: SmallVec<[ParticipantIdentity; 8]> = {
                        let subscriptions = session.subscriptions.read();
                        match subscriptions.get(&msg.channel_id) {
                            Some(ids) => ids.iter().cloned().collect(),
                            None => continue,
                        }
                    };
                    // Get the participants that are subscribed to the channel
                    let participants: SmallVec<[Arc<Participant>; 8]> = {
                        let participants = session.participants.read();
                        subscriber_ids
                            .iter()
                            .filter_map(|id| participants.get(id).cloned())
                            .collect()
                    };
                    for participant in &participants {
                        if let Err(e) = participant.send(&msg.data).await {
                            error!("failed to send message data to {participant:?}: {e:?}");
                        }
                    }
                }
            }
        }
    }

    async fn handle_byte_stream_from_client(
        self: &Arc<Self>,
        participant_identity: ParticipantIdentity,
        mut reader: ByteStreamReader,
    ) {
        let mut buffer = BytesMut::new();
        loop {
            let chunk = match reader.next().await {
                Some(Ok(chunk)) => chunk,
                Some(Err(e)) => {
                    error!(
                        "Error reading from byte stream for client {:?}: {:?}",
                        participant_identity, e
                    );
                    break;
                }
                None => break,
            };

            buffer.extend_from_slice(&chunk);

            // Parse complete messages from buffer
            while buffer.len() >= MESSAGE_FRAME_SIZE {
                // Parse frame header: 1 byte OpCode + 4 bytes little-endian u32 length
                let opcode = buffer[0];
                let length =
                    u32::from_le_bytes(buffer[1..MESSAGE_FRAME_SIZE].try_into().unwrap()) as usize;

                if length > MAX_MESSAGE_SIZE {
                    error!(
                        "message too large ({length} bytes) from client {:?}, disconnecting",
                        participant_identity
                    );
                    return;
                }

                // Check if we have the complete message
                if buffer.len() < MESSAGE_FRAME_SIZE + length {
                    break; // Wait for more data
                }

                // Split off the header (opcode + length) and payload as a single message
                let message = buffer.split_to(MESSAGE_FRAME_SIZE + length);

                // Extract the payload without copying by splitting off the header
                let payload = message.freeze().slice(MESSAGE_FRAME_SIZE..);

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
            _ => {
                warn!("Unhandled client message: {client_msg:?}");
            }
        }
    }

    fn handle_client_subscribe(&self, participant: &Participant, msg: client::Subscribe) {
        let channel_ids: Vec<ChannelId> = msg
            .channel_ids
            .iter()
            .map(|&id| ChannelId::new(id))
            .collect();

        let mut first_subscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        {
            let mut subscriptions = self.subscriptions.write();
            for &channel_id in &channel_ids {
                let subscribers = subscriptions.entry(channel_id).or_default();
                if subscribers.contains(participant.identity()) {
                    info!(
                        "Participant {:?} is already subscribed to channel {channel_id:?}; ignoring",
                        participant
                    );
                    continue;
                }
                let is_first = subscribers.is_empty();
                subscribers.push(participant.identity().clone());
                debug!(
                    "Participant {:?} subscribed to channel {channel_id:?}",
                    participant
                );
                if is_first {
                    first_subscribed.push(channel_id);
                }
            }
        }

        if !first_subscribed.is_empty() {
            if let Some(context) = self.context.upgrade() {
                context.subscribe_channels(self.sink_id, &first_subscribed);
            }
        }
    }

    fn handle_client_unsubscribe(&self, participant: &Participant, msg: client::Unsubscribe) {
        let channel_ids: Vec<ChannelId> = msg
            .channel_ids
            .iter()
            .map(|&id| ChannelId::new(id))
            .collect();

        let mut last_unsubscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        {
            let mut subscriptions = self.subscriptions.write();
            for &channel_id in &channel_ids {
                let Some(subscribers) = subscriptions.get_mut(&channel_id) else {
                    info!(
                        "Participant {:?} is not subscribed to channel {channel_id:?}; ignoring",
                        participant
                    );
                    continue;
                };
                let Some(pos) = subscribers
                    .iter()
                    .position(|id| id == participant.identity())
                else {
                    info!(
                        "Participant {:?} is not subscribed to channel {channel_id:?}; ignoring",
                        participant
                    );
                    continue;
                };
                subscribers.swap_remove(pos);
                debug!(
                    "Participant {:?} unsubscribed from channel {channel_id:?}",
                    participant
                );
                if subscribers.is_empty() {
                    subscriptions.remove(&channel_id);
                    last_unsubscribed.push(channel_id);
                }
            }
        }

        if !last_unsubscribed.is_empty() {
            if let Some(context) = self.context.upgrade() {
                context.unsubscribe_channels(self.sink_id, &last_unsubscribed);
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

    /// Remove a participant from the session, cleaning up its subscriptions.
    ///
    /// Channels that lose their last subscriber are unsubscribed from the context.
    fn remove_participant(&self, participant_id: &ParticipantIdentity) {
        if self.participants.write().remove(participant_id).is_none() {
            return;
        }
        info!("removed participant {participant_id:?}");

        let mut last_unsubscribed: SmallVec<[ChannelId; 4]> = SmallVec::new();
        {
            let mut subscriptions = self.subscriptions.write();
            subscriptions.retain(|&channel_id, subscribers| {
                subscribers.retain(|id| id != participant_id);
                if subscribers.is_empty() {
                    last_unsubscribed.push(channel_id);
                    false
                } else {
                    true
                }
            });
        }

        if !last_unsubscribed.is_empty() {
            if let Some(context) = self.context.upgrade() {
                context.unsubscribe_channels(self.sink_id, &last_unsubscribed);
            }
        }
    }

    /// Enqueue server info and channel advertisements for delivery to a participant.
    ///
    /// Messages are routed through the control plane queue so that all writes to the
    /// participant's ByteStreamWriter are serialized by the sender task. ByteStreamWriter::write
    /// is not safe to call concurrently from multiple tasks.
    fn send_info_and_advertisements(&self, participant: Arc<Participant>, server_info: ServerInfo) {
        info!("sending server info and advertisements to participant {participant:?}");
        let framed = frame_text_message(server_info.to_string().as_bytes());
        self.send_control(participant.clone(), framed);
        self.send_channel_advertisements(participant);
    }

    /// Enqueue all currently cached channel advertisements for delivery to a single participant.
    fn send_channel_advertisements(&self, participant: Arc<Participant>) {
        let framed = {
            let channels = self.channels.read();
            if channels.is_empty() {
                return;
            }
            let advertise_msg = advertise::advertise_channels(channels.values());
            if advertise_msg.channels.is_empty() {
                return;
            }
            frame_text_message(advertise_msg.to_string().as_bytes())
        };

        self.send_control(participant, framed);
    }
}

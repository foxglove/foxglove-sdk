use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};
use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt;
use indexmap::IndexSet;
use libwebrtc::video_source::{RtcVideoSource, native::NativeVideoSource};
use livekit::options::TrackPublishOptions;
use livekit::{ByteStreamReader, Room, StreamByteOptions, id::ParticipantIdentity};
use livekit::{StreamWriter, prelude::*};
use parking_lot::RwLock;
use semver::Version;
use smallvec::SmallVec;
use tokio::io::AsyncReadExt;
use tokio_util::{io::StreamReader, sync::CancellationToken};
use tracing::{debug, error, info, warn};

use crate::protocol::v2::DecodeError;
use crate::protocol::v2::parameter::Parameter;
use crate::protocol::v2::server::ParameterValues;
use crate::remote_access::participant::ChannelWriter;
use crate::remote_common::ClientId;
use crate::remote_common::connection_graph::ConnectionGraph;
use crate::remote_common::{
    fetch_asset::AssetResponder,
    service::{CallId, Service, ServiceId, ServiceMap},
};
use crate::time::millis_since_epoch;
use crate::{
    ChannelDescriptor, ChannelId, Context, FoxgloveError, Metadata, RawChannel, Schema, Sink,
    SinkChannelFilter, SinkId,
    protocol::v2::{
        BinaryMessage, JsonMessage,
        client::{self, ClientMessage},
        server::{
            AdvertiseServices, MessageData as ServerMessageData, Pong, RemoveStatus, ServerInfo,
            ServiceCallFailure, Status, Unadvertise, UnadvertiseServices, advertise,
            advertise_services,
        },
    },
    remote_access::{
        AssetHandler, Capability, Listener, RemoteAccessError, client::Client,
        participant::Participant, protocol_version, rtt_tracker::RttTracker,
        session_state::SessionState,
    },
};

mod video_track;
pub(crate) use video_track::{
    VideoInputSchema, VideoMetadata, VideoPublisher, get_video_input_schema,
};

#[derive(Debug)]
pub(crate) struct SessionStats {
    pub participants: usize,
    pub subscriptions: usize,
    pub video_tracks: usize,
}

const CONTROL_CHANNEL_TOPIC: &str = "control";
const CHANNEL_TOPIC_PREFIX: &str = "device-ch-";
const CLIENT_CHANNEL_TOPIC_PREFIX: &str = "client-ch-";
const MESSAGE_FRAME_SIZE: usize = 5; // 1 byte opcode + u32 LE length
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16 MiB
const MAX_SEND_RETRIES: usize = 3;
pub(crate) const DEFAULT_PENDING_CLIENT_READER_TIMEOUT: Duration = Duration::from_secs(15);

/// A data plane message queued for delivery to subscribed participants.
struct ChannelMessage {
    channel_id: ChannelId,
    data: Bytes,
}

impl ChannelMessage {
    fn binary<'a>(channel_id: ChannelId, message: &impl BinaryMessage<'a>) -> Self {
        Self {
            channel_id,
            data: encode_binary_message(message),
        }
    }
}

/// A control plane message queued for delivery to a specific participant.
pub(super) struct ControlPlaneMessage {
    participant: Arc<Participant>,
    data: Bytes,
}

impl ControlPlaneMessage {
    pub(super) fn json(participant: Arc<Participant>, message: &impl JsonMessage) -> Self {
        Self {
            participant,
            data: encode_json_message(message),
        }
    }

    pub(super) fn binary<'a>(
        participant: Arc<Participant>,
        message: &impl BinaryMessage<'a>,
    ) -> Self {
        Self {
            participant,
            data: encode_binary_message(message),
        }
    }
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

/// Encodes a JSON message with the v2 byte stream framing (1 byte opcode + 4 byte LE length + payload).
fn encode_json_message(message: &impl JsonMessage) -> Bytes {
    let payload = message.to_string();
    let payload = payload.as_bytes();
    let mut buf = Vec::with_capacity(MESSAGE_FRAME_SIZE + payload.len());
    buf.push(OpCode::Text as u8);
    let len = u32::try_from(payload.len()).expect("message too large");
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(payload);
    Bytes::from(buf)
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

fn build_advertise_services_msg(services: &[Arc<Service>]) -> Option<AdvertiseServices<'_>> {
    if services.is_empty() {
        return None;
    }
    let msg = AdvertiseServices::new(services.iter().filter_map(|s| {
        advertise_services::Service::try_from(s.as_ref())
            .inspect_err(|err| {
                error!(
                    "Failed to encode service advertisement for {}: {err}",
                    s.name()
                )
            })
            .ok()
    }));
    if msg.services.is_empty() {
        return None;
    }
    Some(msg)
}

/// RemoteAccessSession tracks a connected LiveKit session (the Room)
/// and any state that is specific to that session.
/// We discard this state if we close or lose the connection.
/// [`super::connection::RemoteAccessConnection`] manages the current connected session (if any)
///
/// The Sink impl is at the RemoteAccessSession level (not per-participant)
/// so that it can deliver messages via multi-cast to multiple participants.
pub(crate) struct RemoteAccessSession {
    sink_id: SinkId,
    room: Room,
    context: Weak<Context>,
    remote_access_session_id: Option<String>,
    state: RwLock<SessionState>,
    channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    listener: Option<Arc<dyn Listener>>,
    capabilities: Vec<Capability>,
    fetch_asset_handler: Option<Arc<dyn AssetHandler<Client>>>,
    cancellation_token: CancellationToken,
    data_plane_tx: flume::Sender<ChannelMessage>,
    data_plane_rx: flume::Receiver<ChannelMessage>,
    control_plane_tx: flume::Sender<ControlPlaneMessage>,
    control_plane_rx: flume::Receiver<ControlPlaneMessage>,
    services: Arc<parking_lot::RwLock<ServiceMap>>,
    supported_encodings: IndexSet<String>,
    /// Serializes all participant-scoped state mutations: subscription changes, video track
    /// lifecycle operations, client channel advertise/unadvertise, and participant removal.
    /// This prevents TOCTOU races between byte-stream message handlers and room-event handlers,
    /// which run on separate tokio tasks.
    subscription_lock: parking_lot::Mutex<()>,
    /// Signaled by video publishers when video metadata changes, prompting
    /// the sender loop to re-advertise affected channels.
    video_metadata_tx: tokio::sync::watch::Sender<()>,
    video_metadata_rx: tokio::sync::watch::Receiver<()>,
    /// Byte stream readers for `client-ch-{channelId}` streams that arrived before the
    /// corresponding Client Advertise message. Keyed by participant identity then channel ID.
    /// Drained when the advertise arrives; expired after `pending_client_reader_timeout`.
    pending_client_readers:
        parking_lot::Mutex<HashMap<ParticipantIdentity, HashMap<ChannelId, ByteStreamReader>>>,
    pending_client_reader_timeout: Duration,
    rtt_tracker: parking_lot::Mutex<RttTracker>,
    ice_rtt_tracker: parking_lot::Mutex<RttTracker>,
    connection_graph: Arc<parking_lot::Mutex<ConnectionGraph>>,
    /// Immutable `ServerInfo` message sent to each participant on connect and reset.
    server_info: ServerInfo,
    /// Channel used by `run_sender` to request participant resets when a control
    /// stream write fails. `handle_room_events` receives identities and calls
    /// `reset_participant`. Using `mpsc` rather than `Notify` because `Notify` is
    /// not cancel-safe in `select!`.
    participant_reset_tx: tokio::sync::mpsc::UnboundedSender<ParticipantIdentity>,
    participant_reset_rx:
        tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<ParticipantIdentity>>,
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

        let state = self.state.read();

        // Send to video publisher if any subscribers requested a video track.
        if let Some(publisher) = state.get_video_publisher(&channel_id) {
            publisher.send(Bytes::copy_from_slice(msg), metadata.log_time);
        }

        // Send to data subscribers.
        if state.has_data_subscribers(&channel_id) {
            drop(state);
            let message = ServerMessageData::new(u64::from(channel_id), metadata.log_time, msg);
            self.send_data_lossy(ChannelMessage::binary(channel_id, &message));
        }

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

        let mut advertise_msg = advertise::advertise_channels(filtered.iter().copied());
        if advertise_msg.channels.is_empty() {
            return None;
        }

        // Track advertised channels and detect video-capable ones.
        let advertised_ids: std::collections::HashSet<u64> =
            advertise_msg.channels.iter().map(|ch| ch.id).collect();
        {
            let mut state = self.state.write();
            for &ch in &filtered {
                if advertised_ids.contains(&u64::from(ch.id())) {
                    state.insert_channel(ch);
                    if let Some(input_schema) = get_video_input_schema(ch) {
                        state.insert_video_schema(ch.id(), input_schema);
                    }
                }
            }
            state.add_metadata_to_advertisement(&mut advertise_msg);
        }

        self.broadcast_control(encode_json_message(&advertise_msg));

        // Clients subscribe asynchronously.
        None
    }

    fn remove_channel(&self, channel: &RawChannel) {
        let _guard = self.subscription_lock.lock();
        let channel_id = channel.id();

        // Collect subscriber info before removal for on_unsubscribe callbacks.
        let subscriber_clients = self.state.read().channel_subscriber_clients(&channel_id);

        if !self.state.write().remove_channel(channel_id) {
            return;
        }

        self.teardown_video_track(channel_id);
        self.state.write().remove_video_schema(&channel_id);

        let unadvertise = Unadvertise::new([u64::from(channel_id)]);
        self.broadcast_control(encode_json_message(&unadvertise));

        // Fire on_unsubscribe callbacks for subscribers of the removed channel.
        if let Some(listener) = &self.listener {
            let descriptor = channel.descriptor();
            for (client_id, participant_id) in subscriber_clients {
                let client = Client::new(client_id, participant_id);
                listener.on_unsubscribe(&client, descriptor);
            }
        }
    }

    fn auto_subscribe(&self) -> bool {
        false
    }
}

pub(crate) struct SessionParams {
    pub room: Room,
    pub context: Weak<Context>,
    pub channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    pub listener: Option<Arc<dyn Listener>>,
    pub capabilities: Vec<Capability>,
    pub supported_encodings: IndexSet<String>,
    pub cancellation_token: CancellationToken,
    pub message_backlog_size: usize,
    pub services: Arc<parking_lot::RwLock<ServiceMap>>,
    pub connection_graph: Arc<parking_lot::Mutex<ConnectionGraph>>,
    pub pending_client_reader_timeout: Duration,
    pub remote_access_session_id: Option<String>,
    pub fetch_asset_handler: Option<Arc<dyn AssetHandler<Client>>>,
    pub server_info: ServerInfo,
}

impl RemoteAccessSession {
    pub(crate) fn new(params: SessionParams) -> Self {
        let (data_plane_tx, data_plane_rx) = flume::bounded(params.message_backlog_size);
        let (control_plane_tx, control_plane_rx) = flume::bounded(params.message_backlog_size);
        let (video_metadata_tx, video_metadata_rx) = tokio::sync::watch::channel(());
        let (participant_reset_tx, participant_reset_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            sink_id: SinkId::next(),
            room: params.room,
            context: params.context,
            remote_access_session_id: params.remote_access_session_id,
            state: RwLock::new(SessionState::new()),
            channel_filter: params.channel_filter,
            listener: params.listener,
            capabilities: params.capabilities,
            fetch_asset_handler: params.fetch_asset_handler,
            cancellation_token: params.cancellation_token,
            data_plane_tx,
            data_plane_rx,
            control_plane_tx,
            control_plane_rx,
            subscription_lock: parking_lot::Mutex::new(()),
            video_metadata_tx,
            video_metadata_rx,
            pending_client_readers: parking_lot::Mutex::new(HashMap::new()),
            pending_client_reader_timeout: params.pending_client_reader_timeout,
            services: params.services,
            supported_encodings: params.supported_encodings,
            rtt_tracker: parking_lot::Mutex::new(RttTracker::new("ping/pong")),
            ice_rtt_tracker: parking_lot::Mutex::new(RttTracker::new("ICE")),
            connection_graph: params.connection_graph,
            server_info: params.server_info,
            participant_reset_tx,
            participant_reset_rx: tokio::sync::Mutex::new(participant_reset_rx),
        }
    }

    /// Returns true if the given capability is enabled for this session.
    fn has_capability(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }

    pub(crate) fn remote_access_session_id(&self) -> Option<&str> {
        self.remote_access_session_id.as_deref()
    }

    pub(crate) fn sink_id(&self) -> SinkId {
        self.sink_id
    }

    pub(crate) fn room(&self) -> &Room {
        &self.room
    }

    pub(crate) fn stats(&self) -> SessionStats {
        let state = self.state.read();
        SessionStats {
            participants: state.participant_count(),
            subscriptions: state.subscription_count(),
            video_tracks: state.video_track_count(),
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

    /// Send an error status message to a participant.
    fn send_error(&self, participant: &Arc<Participant>, message: String) {
        debug!("Sending error to {participant}: {message}");
        let status = Status::error(message);
        self.send_control(participant.clone(), encode_json_message(&status));
    }

    /// Send a warning status message to a participant.
    fn send_warning(&self, participant: &Arc<Participant>, message: String) {
        debug!("Sending warning to {participant}: {message}");
        let status = Status::warning(message);
        self.send_control(participant.clone(), encode_json_message(&status));
    }

    /// Enqueue a control plane message for all currently connected participants.
    fn broadcast_control(&self, data: Bytes) {
        let participants = self.state.read().collect_participants();
        for participant in participants {
            self.send_control(participant, data.clone());
        }
    }

    /// Reads from the data plane and control plane queues and sends messages to participants.
    ///
    /// Control plane messages are sent to the targeted participant via its per-participant writer.
    /// Data plane messages are written to a per-channel `ByteStreamWriter` addressed to the
    /// channel's current subscriber set. The writer is created (or replaced) lazily: if the
    /// locally cached writer's subscription version differs from the current version in state,
    /// the old writer is dropped and a new one is opened for the up-to-date subscriber set.
    pub(crate) async fn run_sender(session: Arc<Self>) {
        let mut channel_writers: HashMap<ChannelId, ChannelWriter> = HashMap::new();
        let mut video_metadata: HashMap<ChannelId, VideoMetadata> = HashMap::new();
        let mut video_metadata_rx = session.video_metadata_rx.clone();
        // Participants whose control stream has broken. Messages for a poisoned
        // `ClientId` are dropped without attempting a write. A fresh `ClientId` is
        // assigned by `add_participant` after reset, so messages for the new
        // participant pass through normally.
        let mut poisoned_participants: HashSet<ClientId> = HashSet::new();
        loop {
            tokio::select! {
                biased;
                () = session.cancellation_token.cancelled() => break,
                msg = session.control_plane_rx.recv_async() => {
                    let Ok(msg) = msg else { break };
                    if poisoned_participants.contains(&msg.participant.client_id()) {
                        continue;
                    }
                    if let Err(e) = msg.participant.send(&msg.data).await {
                        warn!(
                            "control write failed for {:?}, requesting reset: {e:?}",
                            msg.participant,
                        );
                        poisoned_participants.insert(msg.participant.client_id());
                        let _ = session.participant_reset_tx.send(
                            msg.participant.participant_id().clone(),
                        );
                    }
                }
                Ok(()) = video_metadata_rx.changed() => {
                    session
                        .republish_video_metadata(&mut video_metadata);
                }
                msg = session.data_plane_rx.recv_async() => {
                    let Ok(msg) = msg else { break };
                    process_data_message(
                        &session.state,
                        &msg,
                        &mut channel_writers,
                        |channel_id, subscribers, version| {
                            let session = Arc::clone(&session);
                            async move {
                                let topic = format!(
                                    "{CHANNEL_TOPIC_PREFIX}{}",
                                    u64::from(channel_id),
                                );
                                match session
                                    .room
                                    .local_participant()
                                    .stream_bytes(StreamByteOptions {
                                        topic,
                                        destination_identities: subscribers,
                                        ..StreamByteOptions::default()
                                    })
                                    .await
                                {
                                    Ok(s) => Some(ChannelWriter::new(s, version)),
                                    Err(e) => {
                                        error!(
                                            "failed to open byte stream for channel \
                                             {channel_id:?}: {e:?}",
                                        );
                                        None
                                    }
                                }
                            }
                        },
                    )
                    .await;
                }
            }
        }
    }

    /// Read framed messages from a client byte stream.
    ///
    /// `expected_channel_id` identifies a `client-ch-{channelId}` stream: the channel ID parsed from
    /// the topic name. Every `MessageData` frame on this stream must carry the same channel ID;
    /// mismatches are considered a protocol violation (debug-asserted). Pass `None` for the
    /// `"control"` control stream.
    pub(crate) async fn handle_byte_stream_from_client(
        self: &Arc<Self>,
        participant_identity: ParticipantIdentity,
        reader: ByteStreamReader,
        expected_channel_id: Option<u32>,
    ) {
        let stream = reader.map(|result| result.map_err(std::io::Error::other));
        let mut reader = StreamReader::new(stream);

        loop {
            let mut header = [0u8; MESSAGE_FRAME_SIZE];
            let read_result = tokio::select! {
                () = self.cancellation_token.cancelled() => break,
                result = reader.read_exact(&mut header) => result,
            };
            match read_result {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => {
                    error!(
                        "Error reading from byte stream for client {:?}: {:?}",
                        participant_identity, e
                    );
                    break;
                }
            }

            let opcode = header[0];
            let length =
                u32::from_le_bytes(header[1..MESSAGE_FRAME_SIZE].try_into().unwrap()) as usize;

            if length > MAX_MESSAGE_SIZE {
                error!(
                    "message too large ({length} bytes) from client {:?}, disconnecting",
                    participant_identity
                );
                return;
            }

            let mut payload = vec![0u8; length];
            let read_result = tokio::select! {
                () = self.cancellation_token.cancelled() => break,
                result = reader.read_exact(&mut payload) => result,
            };
            match read_result {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => {
                    error!(
                        "Error reading from byte stream for client {:?}: {:?}",
                        participant_identity, e
                    );
                    break;
                }
            }

            if let Some(channel_id) = expected_channel_id {
                if !self.handle_channel_stream_message(
                    &participant_identity,
                    channel_id,
                    opcode,
                    &payload,
                ) {
                    return;
                }
            } else if !self.handle_client_control_message(
                &participant_identity,
                opcode,
                Bytes::from(payload),
            ) {
                return;
            }
        }
    }

    /// Handle an incoming `client-ch-{channelId}` byte stream.
    ///
    /// If the client has already advertised this channel, the stream is read immediately.
    /// Otherwise the reader is stashed until the Client Advertise arrives (or a timeout
    /// expires), letting LiveKit buffer the data in the meantime.
    pub(crate) fn handle_client_channel_stream(
        self: &Arc<Self>,
        participant_identity: ParticipantIdentity,
        channel_id: ChannelId,
        reader: ByteStreamReader,
    ) {
        if !self.has_capability(Capability::ClientPublish) {
            drop(reader);
            warn!(
                "Received client channel stream from {participant_identity:?} but clientPublish capability is not enabled"
            );
            if let Some(participant) = self.state.read().get_participant(&participant_identity) {
                self.send_error(
                    &participant,
                    "Server does not support clientPublish capability".to_string(),
                );
            }
            return;
        }

        // Hold the pending lock across the state check and potential insert to prevent a
        // TOCTOU race with handle_client_advertise. The advertise path inserts the channel
        // into state (releasing the state write lock) *then* acquires this lock to drain
        // pending readers. By holding this lock while we check state, we guarantee that
        // either we see the channel (and process immediately) or the advertise path will
        // see our pending reader (and drain it).
        let mut pending = self.pending_client_readers.lock();
        let has_channel = self
            .state
            .read()
            .get_client_channel(&participant_identity, channel_id)
            .is_some();

        if has_channel {
            drop(pending);
            let session = self.clone();
            let expected_channel_id = u64::from(channel_id) as u32;
            tokio::spawn(async move {
                session
                    .handle_byte_stream_from_client(
                        participant_identity,
                        reader,
                        Some(expected_channel_id),
                    )
                    .await;
            });
            return;
        }

        let map = pending.entry(participant_identity.clone()).or_default();
        if let Some(_old) = map.insert(channel_id, reader) {
            debug!("replacing pending reader for {participant_identity:?} channel {channel_id:?}");
        }
        drop(pending);

        let session = self.clone();
        tokio::spawn(async move {
            tokio::select! {
                () = session.cancellation_token.cancelled() => {}
                () = tokio::time::sleep(session.pending_client_reader_timeout) => {
                    let removed = {
                        let mut pending = session.pending_client_readers.lock();
                        let reader = pending
                            .get_mut(&participant_identity)
                            .and_then(|map| map.remove(&channel_id));
                        if pending.get(&participant_identity).is_some_and(|m| m.is_empty()) {
                            pending.remove(&participant_identity);
                        }
                        reader
                    };
                    if removed.is_some() {
                        if let Some(participant) =
                            session.state.read().get_participant(&participant_identity)
                        {
                            session.send_error(
                                &participant,
                                format!(
                                    "Client has not advertised channel: {}",
                                    u64::from(channel_id),
                                ),
                            );
                        }
                    }
                }
            }
        });
    }

    /// Handle a single framed control channel message. Returns `false` if the byte stream
    /// should be closed (e.g. unrecognized opcode indicating a protocol mismatch).
    fn handle_client_control_message(
        self: &Arc<Self>,
        participant_identity: &ParticipantIdentity,
        opcode: u8,
        payload: Bytes,
    ) -> bool {
        const TEXT: u8 = OpCode::Text as u8;
        const BINARY: u8 = OpCode::Binary as u8;
        let client_msg = match opcode {
            TEXT => match std::str::from_utf8(&payload) {
                Ok(text) => ClientMessage::parse_json(text),
                Err(e) => {
                    error!("Invalid UTF-8 in text message: {e:?}");
                    return true;
                }
            },
            BINARY => ClientMessage::parse_binary(&payload[..]),
            _ => {
                error!(
                    "Unrecognized message opcode ({opcode}) received, you likely need to upgrade to a newer version of the Foxglove SDK"
                );
                return false;
            }
        };

        let client_msg = match client_msg {
            Ok(msg) => msg,
            Err(e) => {
                error!("failed to parse client message: {e:?}");
                return true;
            }
        };

        let Some(participant) = self.state.read().get_participant(participant_identity) else {
            error!("Unknown participant identity: {:?}", participant_identity);
            return false;
        };

        match client_msg {
            ClientMessage::Subscribe(msg) => {
                self.handle_client_subscribe(&participant, msg);
            }
            ClientMessage::Unsubscribe(msg) => {
                self.handle_client_unsubscribe(&participant, msg);
            }
            ClientMessage::Advertise(msg) => {
                self.handle_client_advertise(&participant, msg);
            }
            ClientMessage::Unadvertise(msg) => {
                self.handle_client_unadvertise(&participant, msg);
            }
            ClientMessage::MessageData(_) => {
                error!(
                    "Received MessageData over control channel; MessageData is only supported on channel-specific byte streams"
                );
            }
            ClientMessage::FetchAsset(msg) => {
                self.handle_fetch_asset(&participant, msg.uri, msg.request_id);
            }
            ClientMessage::ServiceCallRequest(req) => {
                self.handle_service_call(&participant, req);
            }
            ClientMessage::GetParameters(msg) => {
                self.handle_get_parameters(&participant, msg.parameter_names, msg.id);
            }
            ClientMessage::SetParameters(msg) => {
                self.handle_set_parameters(&participant, msg.parameters, msg.id);
            }
            ClientMessage::SubscribeParameterUpdates(msg) => {
                self.handle_subscribe_parameter_updates(&participant, msg.parameter_names);
            }
            ClientMessage::UnsubscribeParameterUpdates(msg) => {
                self.handle_unsubscribe_parameter_updates(&participant, msg.parameter_names);
            }
            ClientMessage::Ping(msg) => {
                // Build pong payload: [appTimestamp: u64 LE][deviceTimestamp: u64 LE]
                let mut pong_payload = Vec::with_capacity(16);
                pong_payload.extend_from_slice(&msg.payload[..8]);
                pong_payload.extend_from_slice(&millis_since_epoch().to_le_bytes());
                let pong = Pong::new(&pong_payload);
                let framed = encode_binary_message(&pong);
                self.send_control(participant, framed);
            }
            ClientMessage::PingAck(ack) => {
                let now = millis_since_epoch();
                if now >= ack.device_timestamp {
                    let rtt_ms = (now - ack.device_timestamp) as f64;
                    self.rtt_tracker.lock().record_sample(rtt_ms);
                }
            }
            ClientMessage::SubscribeConnectionGraph => {
                self.handle_connection_graph_subscribe(&participant);
            }
            ClientMessage::UnsubscribeConnectionGraph => {
                self.handle_connection_graph_unsubscribe(&participant);
            }
            _ => {
                warn!("Unhandled client message: {client_msg:?}");
            }
        }
        true
    }

    /// Subscribes the participant to the requested channels and notifies the listener.
    ///
    /// Channels the participant is already subscribed to are silently skipped.
    /// The context is notified only for channels gaining their first subscriber.
    fn handle_client_subscribe(
        self: &Arc<Self>,
        participant: &Arc<Participant>,
        msg: client::Subscribe,
    ) {
        let _guard = self.subscription_lock.lock();

        // Collect new & modified subscriptions.
        //
        // If the client's subscription request is unsatisfiable, reject it with an error status
        // message. Note that when a re-subscription fails, we currently leave the original
        // subscription intact. In the future, we may choose to remove the original subscription.
        let mut channel_ids = SmallVec::<[ChannelId; 4]>::new();
        let mut video_channel_ids = SmallVec::<[ChannelId; 4]>::new();
        let mut data_channel_ids = SmallVec::<[ChannelId; 4]>::new();
        let state = self.state.read();
        for ch in &msg.channels {
            let channel_id = ChannelId::new(ch.id);
            if ch.request_video_track {
                if state.get_video_schema(&channel_id).is_some() {
                    video_channel_ids.push(channel_id);
                } else {
                    self.send_error(
                        participant,
                        format!("Channel {} does not support video transcoding", ch.id),
                    );
                    continue;
                }
            } else {
                data_channel_ids.push(channel_id);
            }
            channel_ids.push(channel_id);
        }
        drop(state);

        let mut state = self.state.write();
        let subscribe_result = state.subscribe(participant, &channel_ids);
        state.subscribe_data(participant, &data_channel_ids);
        state.unsubscribe_data(participant, &video_channel_ids);
        let first_video_subscribed = state.subscribe_video(participant, &video_channel_ids);
        let last_video_unsubscribed = state.unsubscribe_video(participant, &data_channel_ids);
        drop(state);

        if !subscribe_result.first_subscribed.is_empty() {
            if let Some(context) = self.context.upgrade() {
                context.subscribe_channels(self.sink_id, &subscribe_result.first_subscribed);
            }
        }

        self.start_video_tracks(&first_video_subscribed);
        self.stop_video_tracks(&last_video_unsubscribed);

        if let Some(listener) = &self.listener {
            if !subscribe_result.newly_subscribed_descriptors.is_empty() {
                let client = Client::new(
                    participant.client_id(),
                    participant.participant_id().clone(),
                );
                for descriptor in &subscribe_result.newly_subscribed_descriptors {
                    listener.on_subscribe(&client, descriptor);
                }
            }
        }
    }

    /// Unsubscribes the participant from the requested channels and notifies the listener.
    ///
    /// Channels the participant was not subscribed to are silently skipped.
    /// The context is notified only for channels losing their last subscriber.
    fn handle_client_unsubscribe(
        self: &Arc<Self>,
        participant: &Participant,
        msg: client::Unsubscribe,
    ) {
        let _guard = self.subscription_lock.lock();
        let channel_ids: Vec<ChannelId> = msg
            .channel_ids
            .iter()
            .map(|&id| ChannelId::new(id))
            .collect();

        let mut state = self.state.write();
        let unsubscribe_result = state.unsubscribe(participant, &channel_ids);
        state.unsubscribe_data(participant, &channel_ids);
        let last_video_unsubscribed = state.unsubscribe_video(participant, &channel_ids);
        drop(state);

        if !unsubscribe_result.last_unsubscribed.is_empty() {
            if let Some(context) = self.context.upgrade() {
                context.unsubscribe_channels(self.sink_id, &unsubscribe_result.last_unsubscribed);
            }
        }

        self.stop_video_tracks(&last_video_unsubscribed);

        if let Some(listener) = &self.listener {
            if !unsubscribe_result
                .actually_unsubscribed_descriptors
                .is_empty()
            {
                let client = Client::new(
                    participant.client_id(),
                    participant.participant_id().clone(),
                );
                for descriptor in &unsubscribe_result.actually_unsubscribed_descriptors {
                    listener.on_unsubscribe(&client, descriptor);
                }
            }
        }
    }

    fn handle_client_advertise(
        self: &Arc<Self>,
        participant: &Arc<Participant>,
        msg: client::Advertise<'_>,
    ) {
        // Serialize with remove_participant, which also holds this lock. Without it,
        // remove_participant can remove the participant from state between the point where
        // handle_client_message resolves the participant and the point where
        // insert_client_channel asserts its presence, causing a panic.
        let _guard = self.subscription_lock.lock();

        if !self.has_capability(Capability::ClientPublish) {
            self.send_error(
                participant,
                "Server does not support clientPublish capability".to_string(),
            );
            return;
        }

        let client = Client::new(
            participant.client_id(),
            participant.participant_id().clone(),
        );

        for ch in msg.channels {
            let channel_id = ChannelId::new(ch.id.into());

            // Decode the schema, tolerating absent schemas. Even when binary schema
            // data is missing, preserve the schema_name so downstream consumers (e.g.
            // the ROS bridge) can identify the message type.
            let schema = match ch.decode_schema() {
                Ok(data) => Some(Schema {
                    name: ch.schema_name.to_string(),
                    encoding: ch.schema_encoding.as_deref().unwrap_or("").to_string(),
                    data: data.into(),
                }),
                Err(DecodeError::MissingSchema) if !ch.schema_name.is_empty() => Some(Schema {
                    name: ch.schema_name.to_string(),
                    encoding: ch.schema_encoding.as_deref().unwrap_or("").to_string(),
                    data: Vec::new().into(),
                }),
                Err(DecodeError::MissingSchema) => None,
                Err(e) => {
                    warn!(
                        "Failed to decode schema for advertised channel {}: {e:?}",
                        ch.id
                    );
                    self.send_error(
                        participant,
                        format!("Failed to decode schema for channel {}: {e}", ch.id),
                    );
                    continue;
                }
            };

            let descriptor = ChannelDescriptor::new(
                channel_id,
                ch.topic.to_string(),
                ch.encoding.to_string(),
                Default::default(),
                schema,
            );

            let inserted = self
                .state
                .write()
                .insert_client_channel(participant.participant_id(), descriptor.clone());

            if !inserted {
                self.send_warning(
                    participant,
                    format!(
                        "Client is already advertising channel: {}; ignoring advertisement",
                        ch.id
                    ),
                );
                continue;
            }

            if let Some(listener) = &self.listener {
                listener.on_client_advertise(&client, &descriptor);
            }

            // Drain any pending byte stream reader that arrived before this advertise.
            let pending_reader = self
                .pending_client_readers
                .lock()
                .get_mut(participant.participant_id())
                .and_then(|map| map.remove(&channel_id));
            if let Some(reader) = pending_reader {
                let session = self.clone();
                let identity = participant.participant_id().clone();
                let expected_channel_id = u64::from(channel_id) as u32;
                tokio::spawn(async move {
                    session
                        .handle_byte_stream_from_client(identity, reader, Some(expected_channel_id))
                        .await;
                });
            }
        }
    }

    fn handle_client_unadvertise(&self, participant: &Arc<Participant>, msg: client::Unadvertise) {
        // Serialize with remove_participant, which also holds this lock. Without it,
        // remove_participant can race with this method and fire on_client_unadvertise for channels
        // it already cleaned up, causing a double invocation of the listener callback.
        let _guard = self.subscription_lock.lock();

        let client = Client::new(
            participant.client_id(),
            participant.participant_id().clone(),
        );

        for channel_id_raw in msg.channel_ids {
            let channel_id = ChannelId::new(channel_id_raw.into());
            let removed = self
                .state
                .write()
                .remove_client_channel(participant.participant_id(), channel_id);

            match removed {
                None => debug!(
                    "Client is not advertising channel: {channel_id_raw}; ignoring unadvertisement"
                ),
                Some(descriptor) => {
                    if let Some(listener) = &self.listener {
                        listener.on_client_unadvertise(&client, &descriptor);
                    }
                }
            }
        }
    }

    /// Send an incompatible protocol version error to a participant that will not be added to the
    /// session. Opens a one-shot byte stream, writes the error status, and closes it.
    pub(crate) async fn send_incompatible_version_error(
        &self,
        participant_id: &ParticipantIdentity,
        attributes: &std::collections::HashMap<String, String>,
    ) {
        let advertised = attributes
            .get(protocol_version::PROTOCOL_VERSION_ATTRIBUTE)
            .cloned()
            .unwrap_or_else(|| protocol_version::DEFAULT_PROTOCOL_VERSION.to_string());
        let message = format!(
            "Remote access protocol version {} is not compatible with this device (supported: {})",
            advertised,
            protocol_version::REMOTE_ACCESS_PROTOCOL_VERSION,
        );
        error!("{}", message);

        let stream = match self
            .room
            .local_participant()
            .stream_bytes(StreamByteOptions {
                topic: CONTROL_CHANNEL_TOPIC.to_string(),
                destination_identities: vec![participant_id.clone()],
                ..StreamByteOptions::default()
            })
            .await
        {
            Ok(s) => s,
            Err(e) => {
                error!(
                    "failed to open error stream for incompatible participant {participant_id}: {e:?}"
                );
                return;
            }
        };

        let status = Status::error(message);
        if let Err(e) = stream.write(&encode_json_message(&status)).await {
            error!("failed to send incompatible version error to {participant_id}: {e:?}");
        }

        // Close the stream so the client receives the end of stream signal.
        // This is not required, if we just drop it LiveKit will spawn a task
        // to close the stream and send the signal anyway, but it's clearer to make it explicit.
        _ = stream.close().await;
    }

    /// Handle a message from a `client-ch-{channelId}` byte stream.
    ///
    /// Only `MessageData` frames are expected. `expected_channel_id` is the channel ID parsed from
    /// the stream topic and must match the `channel_id` field inside every `MessageData` frame.
    /// A mismatch indicates a misbehaving client (the topic determines which stream carries the
    /// data, but the channel ID inside the message determines which descriptor is used).
    ///
    /// Returns `false` if the stream should be closed (protocol violation that will repeat
    /// for every subsequent frame), `true` to continue reading.
    fn handle_channel_stream_message(
        self: &Arc<Self>,
        participant_identity: &ParticipantIdentity,
        expected_channel_id: u32,
        opcode: u8,
        payload: &[u8],
    ) -> bool {
        const BINARY: u8 = OpCode::Binary as u8;
        if opcode != BINARY {
            error!("Unexpected non-binary message on channel stream (opcode {opcode})");
            return true;
        }
        let msg = match ClientMessage::parse_binary(payload) {
            Ok(ClientMessage::MessageData(msg)) => msg,
            Ok(other) => {
                error!(
                    "Unexpected message on channel stream: {other:?}; only MessageData is supported"
                );
                return true;
            }
            Err(e) => {
                error!("Failed to parse channel stream message: {e:?}");
                return true;
            }
        };
        if expected_channel_id != msg.channel_id {
            error!(
                "MessageData channel_id ({}) does not match the stream topic channel_id ({})",
                msg.channel_id, expected_channel_id,
            );
            if let Some(participant) = self.state.read().get_participant(participant_identity) {
                self.send_error(
                    &participant,
                    format!(
                        "MessageData channel_id ({}) does not match the stream topic ({})",
                        msg.channel_id, expected_channel_id,
                    ),
                );
            }
            return false;
        }
        let Some(participant) = self.state.read().get_participant(participant_identity) else {
            error!("Unknown participant identity: {participant_identity:?}");
            return false;
        };
        self.handle_client_message_data(&participant, msg);
        true
    }

    fn handle_client_message_data(
        &self,
        participant: &Arc<Participant>,
        msg: client::MessageData<'_>,
    ) {
        if !self.has_capability(Capability::ClientPublish) {
            self.send_error(
                participant,
                "Server does not support clientPublish capability".to_string(),
            );
            return;
        }
        let channel_id = ChannelId::new(msg.channel_id.into());
        let descriptor = {
            let state = self.state.read();
            state
                .get_client_channel(participant.participant_id(), channel_id)
                .cloned()
        };
        let Some(descriptor) = descriptor else {
            self.send_error(
                participant,
                format!("Client has not advertised channel: {}", msg.channel_id),
            );
            return;
        };
        if let Some(listener) = &self.listener {
            let client = Client::new(
                participant.client_id(),
                participant.participant_id().clone(),
            );
            listener.on_message_data(&client, &descriptor, &msg.data);
        }
    }

    /// Add a participant to the server, if it hasn't already been added.
    ///
    /// The caller is responsible for ensuring that this method is not called concurrently for the
    /// same participant identity.
    ///
    /// When a participant is added, a ServerInfo message and channel Advertisement messages are
    /// immediately queued for transmission.
    pub(crate) async fn add_participant(
        &self,
        participant_id: ParticipantIdentity,
        protocol_version: Version,
    ) -> Result<(), Box<RemoteAccessError>> {
        use crate::remote_access::participant::ParticipantWriter;

        if self.state.read().has_participant(&participant_id) {
            return Ok(());
        }

        let stream = match self
            .room
            .local_participant()
            .stream_bytes(StreamByteOptions {
                topic: CONTROL_CHANNEL_TOPIC.to_string(),
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
            protocol_version,
            ParticipantWriter::Livekit(stream),
            self.control_plane_tx.clone(),
        ));

        // Send initial messages prior to adding the participant to the state map, to ensure that
        // these are the first messages delivered to the participant. This is safe to do without
        // holding the write lock, because this is a new participant - see below.
        info!("sending server info and advertisements to participant {participant:?}");
        self.send_control(participant.clone(), encode_json_message(&self.server_info));
        self.send_channel_advertisements(participant.clone());
        self.send_service_advertisements(participant.clone());

        // Add the participant to the state map. We assert that this is a new participant, because
        // we validated that it did not exist in the map at the top of this function, and the
        // caller is responsible for ensuring this function is not called concurrently for the same
        // participant identity.
        let did_insert = self
            .state
            .write()
            .insert_participant(participant_id, participant);
        assert!(did_insert);
        Ok(())
    }

    /// Remove a participant from the session, cleaning up its subscriptions.
    ///
    /// Channels that lose their last subscriber are unsubscribed from the context.
    pub(crate) fn remove_participant(self: &Arc<Self>, participant_id: &ParticipantIdentity) {
        let _guard = self.subscription_lock.lock();

        self.pending_client_readers.lock().remove(participant_id);

        let removed = self.state.write().remove_participant(participant_id);

        if !removed.last_unsubscribed.is_empty() {
            if let Some(context) = self.context.upgrade() {
                context.unsubscribe_channels(self.sink_id, &removed.last_unsubscribed);
            }
        }

        self.stop_video_tracks(&removed.last_video_unsubscribed);

        if !removed.last_param_unsubscribed.is_empty() {
            if let Some(listener) = &self.listener {
                listener.on_parameters_unsubscribe(removed.last_param_unsubscribed);
            }
        }

        if let Some(client_id) = removed.client_id {
            if self.has_capability(Capability::ConnectionGraph) {
                let mut graph = self.connection_graph.lock();
                if graph.remove_subscriber(client_id) && !graph.has_subscribers() {
                    if let Some(listener) = &self.listener {
                        listener.on_connection_graph_unsubscribe();
                    }
                }
            }
        }

        if let Some((listener, client_id)) = self.listener.as_ref().zip(removed.client_id) {
            let client = Client::new(client_id, participant_id.clone());

            for descriptor in &removed.subscribed_descriptors {
                listener.on_unsubscribe(&client, descriptor);
            }

            for descriptor in &removed.client_channels {
                listener.on_client_unadvertise(&client, descriptor);
            }
        }
    }

    /// Listen for room events and dispatch them.
    ///
    /// Returns when the room is disconnected or the event stream ends.
    pub(crate) async fn handle_room_events(
        self: &Arc<Self>,
        mut room_events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    ) {
        let remote_access_session_id = self.remote_access_session_id();
        let mut participant_reset_rx = self.participant_reset_rx.lock().await;
        loop {
            tokio::select! {
                event = room_events.recv() => {
                    let Some(event) = event else { break };
                    if !self.handle_room_event(event).await {
                        return;
                    }
                }
                // Reset participants whose control streams have broken. This is
                // the same flow as disconnect + reconnect: remove the old state,
                // then re-add with a fresh stream and fresh advertisements.
                Some(participant_id) = participant_reset_rx.recv() => {
                    self.reset_participant(participant_id).await;
                }
            }
        }
        warn!(
            remote_access_session_id,
            "stopped listening for room events"
        );
    }

    /// Handles a single room event. Returns `true` to keep the event loop running,
    /// or `false` to stop (e.g. on disconnect).
    async fn handle_room_event(self: &Arc<Self>, event: RoomEvent) -> bool {
        let remote_access_session_id = self.remote_access_session_id();
        match event {
            RoomEvent::ParticipantConnected(participant) => {
                info!(
                    remote_access_session_id,
                    participant_identity = %participant.identity(),
                    "participant connected to room (waiting for ParticipantActive)"
                );
            }
            RoomEvent::ParticipantActive(participant) => {
                let participant_identity = participant.identity();
                let Some(version) = protocol_version::check_participant_protocol_version(
                    &participant_identity,
                    &participant.attributes(),
                    remote_access_session_id,
                ) else {
                    self.send_incompatible_version_error(
                        &participant_identity,
                        &participant.attributes(),
                    )
                    .await;
                    return true;
                };
                info!(
                    remote_access_session_id,
                    participant_identity = %participant_identity,
                    version = %version,
                    "participant active in room"
                );
                if let Err(e) = self.add_participant(participant.identity(), version).await {
                    error!(remote_access_session_id, error = %e, "failed to add participant: {e}");
                }
            }
            RoomEvent::ParticipantDisconnected(participant) => {
                info!(
                    remote_access_session_id,
                    participant_identity = %participant.identity(),
                    "participant disconnected from room"
                );
                self.remove_participant(&participant.identity());
            }
            RoomEvent::DataReceived {
                payload: _,
                topic,
                kind: _,
                participant: _,
            } => {
                info!(remote_access_session_id, "data received: {:?}", topic);
            }
            RoomEvent::ByteStreamOpened {
                reader,
                topic,
                participant_identity,
            } => {
                info!(
                    remote_access_session_id,
                    participant_identity = %participant_identity,
                    topic = %topic,
                    "byte stream opened from participant"
                );
                if let Some(reader) = reader.take() {
                    if topic == CONTROL_CHANNEL_TOPIC {
                        let session = self.clone();
                        tokio::spawn(async move {
                            session
                                .handle_byte_stream_from_client(participant_identity, reader, None)
                                .await;
                        });
                    } else if let Some(id_str) = topic.strip_prefix(CLIENT_CHANNEL_TOPIC_PREFIX) {
                        if let Ok(id) = id_str.parse::<u64>() {
                            self.handle_client_channel_stream(
                                participant_identity,
                                ChannelId::new(id),
                                reader,
                            );
                        } else {
                            warn!(
                                "invalid channel id in topic {:?} from {:?}",
                                topic, participant_identity
                            );
                        }
                    } else {
                        warn!(
                            "ignoring unexpected byte stream topic from {:?}: {:?}",
                            participant_identity, topic
                        );
                    }
                }
            }
            RoomEvent::ConnectionStateChanged(state) => {
                info!(
                    remote_access_session_id,
                    state = ?state,
                    "connection state changed"
                );
            }
            RoomEvent::Reconnecting => {
                info!(remote_access_session_id, "reconnecting to room");
            }
            RoomEvent::Reconnected => {
                info!(remote_access_session_id, "reconnected to room");
            }
            RoomEvent::ConnectionQualityChanged {
                quality,
                participant,
            } => {
                info!(
                    remote_access_session_id,
                    participant = %participant.identity(),
                    quality = ?quality,
                    "connection quality changed"
                );
            }
            RoomEvent::TrackSubscriptionFailed {
                participant,
                error,
                track_sid,
            } => {
                warn!(
                    remote_access_session_id,
                    participant = %participant.identity(),
                    track_sid = %track_sid,
                    error = %error,
                    "track subscription failed: {error}"
                );
            }
            RoomEvent::LocalTrackPublished {
                publication,
                track: _,
                participant: _,
            } => {
                info!(
                    remote_access_session_id,
                    track_sid = %publication.sid(),
                    track_name = %publication.name(),
                    "local track published"
                );
            }
            RoomEvent::LocalTrackUnpublished {
                publication,
                participant: _,
            } => {
                info!(
                    remote_access_session_id,
                    track_sid = %publication.sid(),
                    track_name = %publication.name(),
                    "local track unpublished"
                );
            }
            RoomEvent::TrackSubscribed {
                track: _,
                publication,
                participant,
            } => {
                info!(
                    remote_access_session_id,
                    participant = %participant.identity(),
                    track_sid = %publication.sid(),
                    track_name = %publication.name(),
                    "remote track subscribed"
                );
            }
            RoomEvent::TrackUnsubscribed {
                track: _,
                publication,
                participant,
            } => {
                info!(
                    remote_access_session_id,
                    participant = %participant.identity(),
                    track_sid = %publication.sid(),
                    track_name = %publication.name(),
                    "remote track unsubscribed"
                );
            }
            RoomEvent::TrackMuted {
                participant,
                publication,
            } => {
                info!(
                    remote_access_session_id,
                    participant = %participant.identity(),
                    track_sid = %publication.sid(),
                    track_name = %publication.name(),
                    "track muted"
                );
            }
            RoomEvent::TrackUnmuted {
                participant,
                publication,
            } => {
                info!(
                    remote_access_session_id,
                    participant = %participant.identity(),
                    track_sid = %publication.sid(),
                    track_name = %publication.name(),
                    "track unmuted"
                );
            }
            RoomEvent::Disconnected { reason } => {
                info!(
                    remote_access_session_id,
                    reason = reason.as_str_name(),
                    "disconnected from room, will attempt to reconnect"
                );
                return false;
            }
            _ => {
                debug!(remote_access_session_id, "room event: {:?}", event);
            }
        }
        true
    }

    /// Tears down a participant and re-initializes it with a fresh control stream.
    ///
    /// This is the recovery path when a control stream write fails: since in-flight
    /// messages may also have been lost, we remove the participant (cleaning up
    /// subscriptions) and re-add it. This opens a fresh stream and re-sends `ServerInfo`
    /// and all advertisements — identical to the normal disconnect/reconnect flow.
    ///
    /// # Interaction with `ParticipantDisconnected`
    ///
    /// Write failures often coincide with participant disconnection. When that happens,
    /// both a reset notification and a `ParticipantDisconnected` event may be in flight.
    /// We guard against the common case by checking `remote_participants()` before
    /// re-adding: if LiveKit has already removed the participant, we skip the re-add
    /// and let the normal `ParticipantConnected` flow handle any future reconnection.
    ///
    /// This is a best-effort check (TOCTOU): the participant could disconnect between
    /// the check and the `stream_bytes` call inside `add_participant`. In that narrow
    /// window, `add_participant` may open a dead stream, but the subsequent
    /// `ParticipantDisconnected` event will clean it up. This is harmless — just a
    /// wasted `stream_bytes` call and a log line.
    async fn reset_participant(self: &Arc<Self>, participant_id: ParticipantIdentity) {
        let remote_access_session_id = self.remote_access_session_id();

        self.remove_participant(&participant_id);

        // Best-effort guard: skip re-add if LiveKit has already removed the participant
        // (e.g., because the underlying WebRTC connection dropped). In that case, the
        // `ParticipantDisconnected` event is already queued and a future reconnect will
        // go through the normal `ParticipantConnected` → `add_participant` path.
        let remote_participant = self
            .room
            .remote_participants()
            .get(&participant_id)
            .cloned();
        let Some(remote_participant) = remote_participant else {
            info!(
                remote_access_session_id,
                participant_identity = %participant_id,
                "participant already left room, skipping re-add after control stream failure",
            );
            return;
        };

        let Some(version) = protocol_version::check_participant_protocol_version(
            &participant_id,
            &remote_participant.attributes(),
            remote_access_session_id,
        ) else {
            warn!(
                remote_access_session_id,
                participant_identity = %participant_id,
                "skipping reset for participant with incompatible protocol version",
            );
            return;
        };

        warn!(
            remote_access_session_id,
            participant_identity = %participant_id,
            "resetting participant after control stream failure",
        );
        if let Err(e) = self.add_participant(participant_id, version).await {
            error!(
                remote_access_session_id,
                error = %e,
                "failed to re-add participant after reset: {e}",
            );
        }
    }

    /// Periodically logs session statistics for monitoring and debugging.
    pub(crate) async fn log_periodic_stats(&self) {
        let remote_access_session_id = self.remote_access_session_id();
        let period = Duration::from_secs(30);
        let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let stats = self.stats();
            let connection_quality = self.room.local_participant().connection_quality();
            let (total_video_bytes_sent, ice_rtt_ms) = match self.room.get_stats().await {
                Ok(stats) => {
                    let total_video_bytes_sent = stats
                        .publisher_stats
                        .iter()
                        .filter_map(|s| match s {
                            libwebrtc::stats::RtcStats::OutboundRtp(rtp)
                                if rtp.stream.kind == "video" =>
                            {
                                Some(rtp.sent.bytes_sent)
                            }
                            _ => None,
                        })
                        .sum::<u64>();
                    let ice_rtt_ms = stats
                        .publisher_stats
                        .iter()
                        .filter_map(|s| match s {
                            libwebrtc::stats::RtcStats::CandidatePair(cp)
                                if cp.candidate_pair.nominated =>
                            {
                                Some(cp.candidate_pair.current_round_trip_time * 1000.0)
                            }
                            _ => None,
                        })
                        .next();
                    (Some(total_video_bytes_sent), ice_rtt_ms)
                }
                Err(e) => {
                    warn!(remote_access_session_id, error = %e, "failed to get room stats: {e}");
                    (None, None)
                }
            };
            if let Some(rtt_ms) = ice_rtt_ms {
                self.ice_rtt_tracker.lock().record_sample(rtt_ms);
            }
            info!(
                remote_access_session_id,
                participants = stats.participants,
                subscriptions = stats.subscriptions,
                video_tracks = stats.video_tracks,
                total_video_bytes_sent,
                connection_quality = ?connection_quality,
                "periodic stats"
            );
        }
    }

    /// Enqueue all currently cached channel advertisements for delivery to a single participant.
    fn send_channel_advertisements(&self, participant: Arc<Participant>) {
        let Some(advertise_msg) = ({
            let state = self.state.read();
            state
                .with_channels(|channels| {
                    let msg = advertise::advertise_channels(channels.values());
                    if msg.channels.is_empty() {
                        return None;
                    }
                    let mut msg = msg.into_owned();
                    state.add_metadata_to_advertisement(&mut msg);
                    Some(msg)
                })
                .flatten()
        }) else {
            return;
        };

        self.send_control(participant, encode_json_message(&advertise_msg));
    }

    /// Enqueue service advertisements for delivery to a single participant.
    fn send_service_advertisements(&self, participant: Arc<Participant>) {
        let services: Vec<_> = self.services.read().values().cloned().collect();
        if let Some(msg) = build_advertise_services_msg(&services) {
            self.send_control(participant, encode_json_message(&msg));
        }
    }

    /// Broadcasts service advertisements for the given service IDs to all connected participants.
    pub(crate) fn advertise_new_services(&self, service_ids: &[ServiceId]) {
        let services: Vec<_> = {
            let services = self.services.read();
            service_ids
                .iter()
                .filter_map(|id| services.get_by_id(*id))
                .collect()
        };
        if let Some(msg) = build_advertise_services_msg(&services) {
            self.broadcast_control(encode_json_message(&msg));
        }
    }

    /// Broadcasts service unadvertisements for the given service IDs to all connected participants.
    pub(crate) fn unadvertise_services(&self, service_ids: &[ServiceId]) {
        let msg = UnadvertiseServices::new(service_ids.iter().copied().map(u32::from));
        self.broadcast_control(encode_json_message(&msg));
    }

    /// Handle a service call request from a client.
    fn handle_service_call(&self, participant: &Arc<Participant>, req: client::ServiceCallRequest) {
        let service_id = ServiceId::new(req.service_id);
        let call_id = CallId::new(req.call_id);

        if !self.has_capability(Capability::Services) {
            self.send_service_call_failure(
                participant,
                service_id,
                call_id,
                "Server does not support services",
            );
            return;
        }

        // Lookup the requested service handler.
        let Some(service) = self.services.read().get_by_id(service_id) else {
            self.send_service_call_failure(participant, service_id, call_id, "Unknown service");
            return;
        };

        // If this service declared a request encoding, ensure that it matches. Otherwise, ensure
        // that the request encoding is in the server's global list of supported encodings.
        if !service
            .request_encoding()
            .map(|e| e == req.encoding.as_ref())
            .unwrap_or_else(|| self.supported_encodings.contains(req.encoding.as_ref()))
        {
            self.send_service_call_failure(
                participant,
                service_id,
                call_id,
                "Unsupported encoding",
            );
            return;
        }

        // Acquire the semaphore, or reject if there are too many concurrent requests.
        let Some(guard) = participant.service_call_sem().try_acquire() else {
            self.send_service_call_failure(participant, service_id, call_id, "Too many requests");
            return;
        };

        let encoding = service
            .response_encoding()
            .unwrap_or(req.encoding.as_ref())
            .to_string();

        let responder = super::service::new_responder(
            participant.clone(),
            service_id,
            call_id,
            encoding,
            participant.control_plane_tx().clone(),
            guard,
        );
        let request = crate::remote_common::service::Request::new(
            service.clone(),
            participant.client_id(),
            call_id,
            req.encoding.into_owned(),
            req.payload.into_owned().into(),
        );

        service.call(request, responder);
    }

    /// Sends a service call failure message to a participant.
    fn send_service_call_failure(
        &self,
        participant: &Arc<Participant>,
        service_id: ServiceId,
        call_id: CallId,
        message: &str,
    ) {
        let failure = ServiceCallFailure {
            service_id: service_id.into(),
            call_id: call_id.into(),
            message: message.to_string(),
        };
        self.send_control(participant.clone(), encode_json_message(&failure));
    }

    /// Handle a fetch asset request from a client.
    fn handle_fetch_asset(&self, participant: &Arc<Participant>, uri: String, request_id: u32) {
        if !self.has_capability(Capability::Assets) {
            self.send_error(
                participant,
                "Server does not support assets capability".to_string(),
            );
            return;
        }

        let Some(guard) = participant.fetch_asset_sem().try_acquire() else {
            participant.send_asset_error("Too many concurrent fetch asset requests", request_id);
            return;
        };

        let handler = self.fetch_asset_handler.as_ref().expect(
            "Gateway advertised the Assets capability without providing a handler; \
             this should have been caught in Gateway::start()",
        );
        let client = Client::with_sender(
            participant.client_id(),
            participant.participant_id().clone(),
            participant,
        );
        let responder = AssetResponder::new(client, request_id, guard);
        handler.fetch(uri, responder);
    }

    /// Handle a `GetParameters` request from a client.
    fn handle_get_parameters(
        &self,
        participant: &Arc<Participant>,
        param_names: Vec<String>,
        request_id: Option<String>,
    ) {
        if !self.has_capability(Capability::Parameters) {
            self.send_error(
                participant,
                "Server does not support parameters capability".into(),
            );
            return;
        }

        if let Some(listener) = self.listener.as_ref() {
            let client = Client::new(
                participant.client_id(),
                participant.participant_id().clone(),
            );
            let parameters =
                listener.on_get_parameters(&client, param_names, request_id.as_deref());
            self.send_parameter_values(participant, parameters, request_id);
        }
    }

    /// Handle a `SetParameters` request from a client.
    fn handle_set_parameters(
        &self,
        participant: &Arc<Participant>,
        parameters: Vec<Parameter>,
        request_id: Option<String>,
    ) {
        if !self.has_capability(Capability::Parameters) {
            self.send_error(
                participant,
                "Server does not support parameters capability".into(),
            );
            return;
        }

        let updated_parameters = if let Some(listener) = self.listener.as_ref() {
            let client = Client::new(
                participant.client_id(),
                participant.participant_id().clone(),
            );
            let updated = listener.on_set_parameters(&client, parameters, request_id.as_deref());

            // Send the updated parameters back to the requesting client if `request_id` is set.
            if request_id.is_some() {
                self.send_parameter_values(participant, updated.clone(), request_id);
            }
            updated
        } else {
            parameters
        };
        self.publish_parameter_values(updated_parameters);
    }

    /// Handle a `SubscribeParameterUpdates` request from a client.
    fn handle_subscribe_parameter_updates(
        &self,
        participant: &Arc<Participant>,
        names: Vec<String>,
    ) {
        if !self.has_capability(Capability::Parameters) {
            self.send_error(
                participant,
                "Server does not support parametersSubscribe capability".into(),
            );
            return;
        }
        let _guard = self.subscription_lock.lock();
        let new_names = self
            .state
            .write()
            .subscribe_parameters(participant.participant_id(), names);
        if !new_names.is_empty() {
            if let Some(listener) = &self.listener {
                listener.on_parameters_subscribe(new_names);
            }
        }
    }

    /// Handle an `UnsubscribeParameterUpdates` request from a client.
    fn handle_unsubscribe_parameter_updates(
        &self,
        participant: &Arc<Participant>,
        names: Vec<String>,
    ) {
        if !self.has_capability(Capability::Parameters) {
            self.send_error(
                participant,
                "Server does not support parametersSubscribe capability".into(),
            );
            return;
        }
        let _guard = self.subscription_lock.lock();
        let old_names = self
            .state
            .write()
            .unsubscribe_parameters(participant.participant_id(), names);
        if !old_names.is_empty() {
            if let Some(listener) = &self.listener {
                listener.on_parameters_unsubscribe(old_names);
            }
        }
    }

    /// Send a `ParameterValues` message to a specific participant.
    fn send_parameter_values(
        &self,
        participant: &Arc<Participant>,
        parameters: Vec<Parameter>,
        request_id: Option<String>,
    ) {
        let mut msg = ParameterValues::new(parameters.into_iter().filter(|p| p.value.is_some()));
        if let Some(id) = request_id {
            msg = msg.with_id(id);
        }
        self.send_control(participant.clone(), encode_json_message(&msg));
    }

    /// Publish parameter values to all participants subscribed to those parameters.
    pub(crate) fn publish_parameter_values(&self, parameters: Vec<Parameter>) {
        if !self.has_capability(Capability::Parameters) {
            error!("Server does not support parameters capability");
            return;
        }

        let state = self.state.read();
        let participants = state.collect_participants();
        for participant in &participants {
            // Filter parameters by this participant's subscriptions.
            let filtered: Vec<_> = parameters
                .iter()
                .filter(|p| {
                    state
                        .parameter_subscribers(&p.name)
                        .is_some_and(|ids| ids.contains(participant.participant_id()))
                })
                .cloned()
                .collect();

            if !filtered.is_empty() {
                let no_request_id = None;
                self.send_parameter_values(participant, filtered, no_request_id);
            }
        }
    }

    /// Publish a status message to all connected participants.
    pub(crate) fn publish_status(&self, status: Status) {
        self.broadcast_control(encode_json_message(&status));
    }

    /// Remove status messages by id from all connected participants.
    pub(crate) fn remove_status(&self, status_ids: Vec<String>) {
        let message = RemoveStatus::new(status_ids);
        self.broadcast_control(encode_json_message(&message));
    }

    /// Handle a `SubscribeConnectionGraph` message from a client.
    fn handle_connection_graph_subscribe(&self, participant: &Arc<Participant>) {
        if !self.has_capability(Capability::ConnectionGraph) {
            self.send_error(
                participant,
                "Server does not support connection graph capability".to_string(),
            );
            return;
        }

        let mut graph = self.connection_graph.lock();
        let first = !graph.has_subscribers();
        if !graph.add_subscriber(participant.client_id()) {
            debug!(
                "Participant {} is already subscribed to connection graph updates",
                participant,
            );
            return;
        }

        if first {
            if let Some(listener) = &self.listener {
                listener.on_connection_graph_subscribe();
            }
        }

        let initial_update = graph.as_initial_update();
        self.send_control(participant.clone(), encode_json_message(&initial_update));
    }

    /// Handle an `UnsubscribeConnectionGraph` message from a client.
    fn handle_connection_graph_unsubscribe(&self, participant: &Arc<Participant>) {
        if !self.has_capability(Capability::ConnectionGraph) {
            self.send_error(
                participant,
                "Server does not support connection graph capability".to_string(),
            );
            return;
        }

        let mut graph = self.connection_graph.lock();
        if !graph.remove_subscriber(participant.client_id()) {
            debug!(
                "Participant {} is already unsubscribed from connection graph updates",
                participant,
            );
            return;
        }

        if !graph.has_subscribers() {
            if let Some(listener) = &self.listener {
                listener.on_connection_graph_unsubscribe();
            }
        }
    }

    /// Replaces the connection graph and sends updates to subscribed participants.
    pub(crate) fn replace_connection_graph(&self, replacement_graph: ConnectionGraph) {
        let mut graph = self.connection_graph.lock();
        let update = graph.update(replacement_graph);
        let encoded = encode_json_message(&update);
        let participants = self.state.read().collect_participants();
        for participant in participants {
            if graph.is_subscriber(participant.client_id()) {
                self.send_control(participant, encoded.clone());
            }
        }
    }

    /// Check video publishers for metadata changes and re-advertise affected channels.
    ///
    /// Called from `run_sender` when `video_metadata_rx` signals a change. Compares each
    /// publisher's current metadata against what was last advertised, updates session state for
    /// any changes, and broadcasts re-advertise messages to participants.
    fn republish_video_metadata(&self, advertised: &mut HashMap<ChannelId, VideoMetadata>) {
        // Collect channels whose video metadata has changed.
        let changed: SmallVec<[ChannelId; 4]> = {
            let state = self.state.read();
            state
                .iter_video_publishers()
                .filter_map(|(&channel_id, publisher)| {
                    let guard = publisher.metadata();
                    let current = guard.as_deref()?;
                    if advertised.get(&channel_id) == Some(current) {
                        return None;
                    }
                    advertised.insert(channel_id, current.clone());
                    Some(channel_id)
                })
                .collect()
        };
        if changed.is_empty() {
            return;
        }

        // Update session state and build the re-advertise message.
        let advertise_msg = {
            let mut state = self.state.write();
            // Only insert metadata for channels that still exist, guarding against
            // a channel being removed between the read and write locks.
            for &channel_id in &changed {
                if let Some(meta) = advertised.get(&channel_id)
                    && state.has_channel(&channel_id)
                {
                    state.insert_video_metadata(channel_id, meta.clone());
                }
            }
            state.with_channels(|channels| {
                let chans = changed.iter().filter_map(|id| channels.get(id));
                let msg = advertise::advertise_channels(chans);
                if msg.channels.is_empty() {
                    return None;
                }
                let mut msg = msg.into_owned();
                state.add_metadata_to_advertisement(&mut msg);
                Some(msg)
            })
        };

        if let Some(Some(msg)) = advertise_msg {
            self.broadcast_control(encode_json_message(&msg));
        }
    }

    /// Start video tracks for first-subscribed channels that have video schemas.
    ///
    /// Caller must hold `subscription_lock`.
    fn start_video_tracks(self: &Arc<Self>, first_subscribed: &[ChannelId]) {
        // Collect video-capable channels and their topics while holding the read lock.
        let to_start: SmallVec<[(ChannelId, VideoInputSchema, String); 4]> = {
            let state = self.state.read();
            state
                .with_channels(|channels| {
                    first_subscribed
                        .iter()
                        .filter_map(|&channel_id| {
                            let input_schema = state.get_video_schema(&channel_id)?;
                            let topic = channels
                                .get(&channel_id)
                                .map(|ch| ch.topic().to_string())
                                .unwrap_or_default();
                            Some((channel_id, input_schema, topic))
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

        for (channel_id, input_schema, topic) in to_start {
            let video_source = NativeVideoSource::default();
            let publisher = Arc::new(VideoPublisher::new(
                video_source.clone(),
                input_schema,
                self.video_metadata_tx.clone(),
            ));
            let expected_publisher = publisher.clone();

            self.state
                .write()
                .insert_video_publisher(channel_id, publisher);

            let track =
                LocalVideoTrack::create_video_track(&topic, RtcVideoSource::Native(video_source));

            let local_participant = self.room.local_participant().clone();
            let session = self.clone();
            tokio::spawn(async move {
                let local_track = LocalTrack::Video(track);
                match local_participant
                    .publish_track(local_track, TrackPublishOptions::default())
                    .await
                {
                    Ok(publication) => {
                        let sid = publication.sid();
                        debug!("published video track {sid} for channel {channel_id:?}");
                        // Only store the SID if the publisher in state is still the
                        // one we created. A teardown+resubscribe cycle could have
                        // replaced it with a different publisher.
                        let store = {
                            let mut state = session.state.write();
                            let is_ours = state
                                .get_video_publisher(&channel_id)
                                .is_some_and(|p| Arc::ptr_eq(&p, &expected_publisher));
                            if is_ours {
                                state.insert_video_track_sid(channel_id, sid.clone());
                            }
                            is_ours
                        };
                        if !store {
                            debug!(
                                "video track {sid} for channel {channel_id:?} was torn down during publish; unpublishing"
                            );
                            if let Err(e) = local_participant.unpublish_track(&sid).await {
                                error!("failed to unpublish orphaned video track {sid}: {e:?}");
                            }
                        }
                    }
                    Err(e) => {
                        error!("failed to publish video track for channel {channel_id:?}: {e:?}");
                    }
                }
            });
        }
    }

    /// Stop video tracks for last-unsubscribed channels.
    ///
    /// Caller must hold `subscription_lock`.
    fn stop_video_tracks(self: &Arc<Self>, last_unsubscribed: &[ChannelId]) {
        for &channel_id in last_unsubscribed {
            self.teardown_video_track(channel_id);
        }
    }

    /// Clean up video runtime state for a single channel: remove publisher, remove and unpublish
    /// track. Does not remove the video schema or metadata, which persist for the lifetime of
    /// the channel.
    ///
    /// Caller must hold `subscription_lock`.
    fn teardown_video_track(&self, channel_id: ChannelId) {
        let sid = {
            let mut state = self.state.write();
            // Removing the publisher drops it, which closes the mpsc channel and
            // terminates the background processing task.
            state.remove_video_publisher(&channel_id);
            state.remove_video_track_sid(&channel_id)
        };

        if let Some(sid) = sid {
            let local_participant = self.room.local_participant().clone();
            tokio::spawn(async move {
                if let Err(e) = local_participant.unpublish_track(&sid).await {
                    error!("failed to unpublish video track {sid}: {e:?}");
                } else {
                    debug!("unpublished video track {sid} for channel {channel_id:?}");
                }
            });
        }
    }
}

/// Returns a reference to the locally cached `ChannelWriter` for `channel_id`,
/// creating or replacing it if the subscription version has changed.
///
/// `open_stream` is called to create a new writer when the cached version is stale
/// or no writer exists yet. It receives `(channel_id, subscribers, version)` and
/// returns `Some(writer)` on success or `None` on failure.
///
/// Returns `None` if the channel has no subscribers or if stream creation fails.
async fn get_or_replace_channel_writer<'a, F, Fut>(
    state: &RwLock<SessionState>,
    channel_id: &ChannelId,
    channel_writers: &'a mut HashMap<ChannelId, ChannelWriter>,
    open_stream: F,
) -> Option<&'a ChannelWriter>
where
    F: FnOnce(ChannelId, Vec<ParticipantIdentity>, u32) -> Fut,
    Fut: std::future::Future<Output = Option<ChannelWriter>>,
{
    // Read the current subscription version (fast read-lock, no await).
    let (current_version, subscribers) = {
        let state = state.read();
        // Only consider data subscribers.
        let Some(sub) = state.get_data_subscription(channel_id) else {
            channel_writers.remove(channel_id);
            return None;
        };
        debug_assert!(!sub.subscribers().is_empty());
        let cached_version = channel_writers.get(channel_id).map(|w| w.version());
        if cached_version == Some(sub.version()) {
            // Fast path: writer is up to date.
            return channel_writers.get(channel_id);
        }
        (sub.version(), sub.subscribers().to_vec())
    };

    // Subscriber set changed (or no writer yet): open a new byte stream.
    // The old writer is implicitly closed when it is replaced in the map.
    match open_stream(*channel_id, subscribers, current_version).await {
        Some(writer) => {
            channel_writers.insert(*channel_id, writer);
            channel_writers.get(channel_id)
        }
        None => {
            channel_writers.remove(channel_id);
            None
        }
    }
}

/// Processes a single data plane message: looks up (or creates) the channel writer
/// and writes the message data through it.
///
/// On write failure the writer is removed from the cache so the next message
/// triggers stream re-creation.
async fn process_data_message<F, Fut>(
    state: &RwLock<SessionState>,
    msg: &ChannelMessage,
    channel_writers: &mut HashMap<ChannelId, ChannelWriter>,
    open_stream: F,
) where
    F: FnOnce(ChannelId, Vec<ParticipantIdentity>, u32) -> Fut,
    Fut: std::future::Future<Output = Option<ChannelWriter>>,
{
    let writer =
        get_or_replace_channel_writer(state, &msg.channel_id, channel_writers, open_stream).await;
    let Some(writer) = writer else {
        return;
    };
    if let Err(e) = writer.write(&msg.data).await {
        error!(
            "failed to send data for channel {:?}: {e:?}",
            msg.channel_id
        );
        channel_writers.remove(&msg.channel_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::v2::server::FetchAssetResponse;
    use crate::remote_access::participant::{
        ParticipantWriter, TestByteStreamWriter, TestChannelWriter,
    };
    use crate::remote_common::fetch_asset::{
        AssetHandler, AsyncAssetHandlerFn, BlockingAssetHandlerFn,
    };

    fn make_participant(name: &str) -> (ParticipantIdentity, Arc<Participant>) {
        let identity = ParticipantIdentity(name.to_string());
        let writer = Arc::new(TestByteStreamWriter::default());
        let version = protocol_version::REMOTE_ACCESS_PROTOCOL_VERSION.clone();
        let (tx, _rx) = flume::bounded(16);
        let participant = Arc::new(Participant::new(
            identity.clone(),
            version,
            ParticipantWriter::Test(writer),
            tx,
        ));
        (identity, participant)
    }

    fn make_participant_with_rx(
        name: &str,
    ) -> (Arc<Participant>, flume::Receiver<ControlPlaneMessage>) {
        let identity = ParticipantIdentity(name.to_string());
        let writer = Arc::new(TestByteStreamWriter::default());
        let version = protocol_version::REMOTE_ACCESS_PROTOCOL_VERSION.clone();
        let (tx, rx) = flume::bounded(16);
        let participant = Arc::new(Participant::new(
            identity,
            version,
            ParticipantWriter::Test(writer),
            tx,
        ));
        (participant, rx)
    }

    fn test_client(participant: &Arc<Participant>) -> Client {
        Client::with_sender(
            participant.client_id(),
            participant.participant_id().clone(),
            participant,
        )
    }

    /// A factory that produces a `ChannelWriter` backed by the given test writer.
    fn test_factory(
        writer: Arc<TestChannelWriter>,
    ) -> impl FnOnce(ChannelId, Vec<ParticipantIdentity>, u32) -> std::future::Ready<Option<ChannelWriter>>
    {
        move |_channel_id, _subscribers, version| {
            std::future::ready(Some(ChannelWriter::test(writer, version)))
        }
    }

    /// A factory that always fails to open a stream.
    fn failing_factory()
    -> impl FnOnce(ChannelId, Vec<ParticipantIdentity>, u32) -> std::future::Ready<Option<ChannelWriter>>
    {
        |_channel_id, _subscribers, _version| std::future::ready(None)
    }

    #[tokio::test]
    async fn data_message_writes_to_channel() {
        let state = RwLock::new(SessionState::new());
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);
        state.write().subscribe_data(&p, &[ch]);

        let test_writer = Arc::new(TestChannelWriter::default());
        let mut writers = HashMap::new();

        let msg = ChannelMessage {
            channel_id: ch,
            data: Bytes::from_static(b"hello"),
        };
        process_data_message(
            &state,
            &msg,
            &mut writers,
            test_factory(test_writer.clone()),
        )
        .await;

        assert_eq!(test_writer.writes(), vec![Bytes::from_static(b"hello")]);
        assert!(writers.contains_key(&ch));
    }

    #[tokio::test]
    async fn cached_writer_reused_on_version_match() {
        let state = RwLock::new(SessionState::new());
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);
        state.write().subscribe_data(&p, &[ch]);

        let test_writer = Arc::new(TestChannelWriter::default());
        let mut writers = HashMap::new();

        let msg1 = ChannelMessage {
            channel_id: ch,
            data: Bytes::from_static(b"msg1"),
        };
        process_data_message(
            &state,
            &msg1,
            &mut writers,
            test_factory(test_writer.clone()),
        )
        .await;

        // Second message should reuse the cached writer (factory not called).
        let other_writer = Arc::new(TestChannelWriter::default());
        let msg2 = ChannelMessage {
            channel_id: ch,
            data: Bytes::from_static(b"msg2"),
        };
        process_data_message(
            &state,
            &msg2,
            &mut writers,
            test_factory(other_writer.clone()),
        )
        .await;

        assert_eq!(
            test_writer.writes(),
            vec![Bytes::from_static(b"msg1"), Bytes::from_static(b"msg2")]
        );
        assert!(other_writer.writes().is_empty());
    }

    #[tokio::test]
    async fn writer_replaced_on_subscriber_change() {
        let state = RwLock::new(SessionState::new());
        let (_id_a, pa) = make_participant("alice");
        let (_id_b, pb) = make_participant("bob");
        let ch = ChannelId::new(1);
        state.write().subscribe_data(&pa, &[ch]);

        let writer1 = Arc::new(TestChannelWriter::default());
        let mut writers = HashMap::new();

        let msg1 = ChannelMessage {
            channel_id: ch,
            data: Bytes::from_static(b"before"),
        };
        process_data_message(&state, &msg1, &mut writers, test_factory(writer1.clone())).await;

        // Adding a subscriber bumps the version, so the next message creates a new writer.
        state.write().subscribe_data(&pb, &[ch]);

        let writer2 = Arc::new(TestChannelWriter::default());
        let msg2 = ChannelMessage {
            channel_id: ch,
            data: Bytes::from_static(b"after"),
        };
        process_data_message(&state, &msg2, &mut writers, test_factory(writer2.clone())).await;

        assert_eq!(writer1.writes(), vec![Bytes::from_static(b"before")]);
        assert_eq!(writer2.writes(), vec![Bytes::from_static(b"after")]);
    }

    #[tokio::test]
    async fn no_subscribers_skips_write() {
        let state = RwLock::new(SessionState::new());
        let mut writers = HashMap::new();
        let ch = ChannelId::new(1);

        let factory_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fc = factory_called.clone();

        let msg = ChannelMessage {
            channel_id: ch,
            data: Bytes::from_static(b"nobody"),
        };
        process_data_message(&state, &msg, &mut writers, move |_id, _subs, _v| {
            fc.store(true, std::sync::atomic::Ordering::Relaxed);
            std::future::ready(None)
        })
        .await;

        assert!(!factory_called.load(std::sync::atomic::Ordering::Relaxed));
        assert!(!writers.contains_key(&ch));
    }

    #[tokio::test]
    async fn write_failure_removes_writer_from_cache() {
        let state = RwLock::new(SessionState::new());
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);
        state.write().subscribe_data(&p, &[ch]);

        let failing = Arc::new(TestChannelWriter::new_failing());
        let mut writers = HashMap::new();

        let msg = ChannelMessage {
            channel_id: ch,
            data: Bytes::from_static(b"will fail"),
        };
        process_data_message(&state, &msg, &mut writers, test_factory(failing)).await;

        assert!(
            !writers.contains_key(&ch),
            "writer should be evicted from cache after write failure"
        );
    }

    #[tokio::test]
    async fn writer_replaced_after_unsubscribe_resubscribe() {
        let state = RwLock::new(SessionState::new());
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);
        state.write().subscribe_data(&p, &[ch]);

        let writer1 = Arc::new(TestChannelWriter::default());
        let mut writers = HashMap::new();

        // First message creates a writer.
        let msg1 = ChannelMessage {
            channel_id: ch,
            data: Bytes::from_static(b"before"),
        };
        process_data_message(&state, &msg1, &mut writers, test_factory(writer1.clone())).await;
        assert_eq!(writer1.writes(), vec![Bytes::from_static(b"before")]);

        // Unsubscribe and resubscribe — the version counter is preserved,
        // so the new version differs from the cached writer's version.
        state.write().unsubscribe_data(&p, &[ch]);
        state.write().subscribe_data(&p, &[ch]);

        // Second message should create a NEW writer (not reuse the old one).
        let writer2 = Arc::new(TestChannelWriter::default());
        let msg2 = ChannelMessage {
            channel_id: ch,
            data: Bytes::from_static(b"after"),
        };
        process_data_message(&state, &msg2, &mut writers, test_factory(writer2.clone())).await;

        assert_eq!(writer1.writes(), vec![Bytes::from_static(b"before")]);
        assert_eq!(writer2.writes(), vec![Bytes::from_static(b"after")]);
    }

    #[tokio::test]
    async fn stream_open_failure_does_not_cache_writer() {
        let state = RwLock::new(SessionState::new());
        let (_id, p) = make_participant("alice");
        let ch = ChannelId::new(1);
        state.write().subscribe_data(&p, &[ch]);

        let mut writers = HashMap::new();

        let msg = ChannelMessage {
            channel_id: ch,
            data: Bytes::from_static(b"no stream"),
        };
        process_data_message(&state, &msg, &mut writers, failing_factory()).await;

        assert!(
            !writers.contains_key(&ch),
            "no writer should be cached when stream creation fails"
        );
    }

    // ---- fetch asset tests ----

    #[test]
    fn asset_responder_sends_ok_response() {
        let (participant, rx) = make_participant_with_rx("alice");
        let guard = participant.fetch_asset_sem().try_acquire().unwrap();
        let responder = AssetResponder::new(test_client(&participant), 42, guard);
        responder.respond_ok(b"hello world");

        let msg = rx.try_recv().unwrap();
        assert_eq!(
            msg.data,
            encode_binary_message(&FetchAssetResponse::asset_data(42, &b"hello world"[..]))
        );
    }

    #[test]
    fn asset_responder_sends_error_response() {
        let (participant, rx) = make_participant_with_rx("alice");
        let guard = participant.fetch_asset_sem().try_acquire().unwrap();
        let responder = AssetResponder::new(test_client(&participant), 42, guard);
        responder.respond_err("something went wrong");

        let msg = rx.try_recv().unwrap();
        assert_eq!(
            msg.data,
            encode_binary_message(&FetchAssetResponse::error_message(
                42,
                "something went wrong"
            ))
        );
    }

    #[test]
    fn asset_responder_sends_error_on_drop_without_response() {
        let (participant, rx) = make_participant_with_rx("alice");
        let guard = participant.fetch_asset_sem().try_acquire().unwrap();
        let responder = AssetResponder::new(test_client(&participant), 42, guard);
        drop(responder);

        let msg = rx.try_recv().unwrap();
        assert_eq!(
            msg.data,
            encode_binary_message(&FetchAssetResponse::error_message(
                42,
                "Internal server error: asset handler failed to send a response"
            ))
        );
    }

    #[test]
    fn fetch_asset_semaphore_limits_concurrent_requests() {
        let (participant, rx) = make_participant_with_rx("alice");
        let mut guards = Vec::new();
        while let Some(guard) = participant.fetch_asset_sem().try_acquire() {
            guards.push(guard);
        }
        assert!(participant.fetch_asset_sem().try_acquire().is_none());

        participant.send_asset_error("Too many concurrent fetch asset requests", 99);

        let msg = rx.try_recv().unwrap();
        assert_eq!(
            msg.data,
            encode_binary_message(&FetchAssetResponse::error_message(
                99,
                "Too many concurrent fetch asset requests"
            ))
        );

        guards.pop();
        assert!(participant.fetch_asset_sem().try_acquire().is_some());
    }

    #[test]
    fn asset_responder_releases_semaphore_on_respond() {
        let (participant, _rx) = make_participant_with_rx("alice");
        let mut guards = Vec::new();
        while let Some(guard) = participant.fetch_asset_sem().try_acquire() {
            guards.push(guard);
        }
        let guard = guards.pop().unwrap();
        let responder = AssetResponder::new(test_client(&participant), 1, guard);

        assert!(participant.fetch_asset_sem().try_acquire().is_none());
        responder.respond_ok(b"data");
        assert!(participant.fetch_asset_sem().try_acquire().is_some());
    }

    #[test]
    fn asset_responder_releases_semaphore_on_drop() {
        let (participant, _rx) = make_participant_with_rx("alice");
        let mut guards = Vec::new();
        while let Some(guard) = participant.fetch_asset_sem().try_acquire() {
            guards.push(guard);
        }
        let guard = guards.pop().unwrap();
        let responder = AssetResponder::new(test_client(&participant), 1, guard);

        assert!(participant.fetch_asset_sem().try_acquire().is_none());
        drop(responder);
        assert!(participant.fetch_asset_sem().try_acquire().is_some());
    }

    #[test]
    fn missing_handler_sends_asset_error() {
        let (participant, rx) = make_participant_with_rx("alice");
        participant.send_asset_error("Server does not have a fetch asset handler", 42);

        let msg = rx.try_recv().unwrap();
        assert_eq!(
            msg.data,
            encode_binary_message(&FetchAssetResponse::error_message(
                42,
                "Server does not have a fetch asset handler"
            ))
        );
    }

    #[tokio::test]
    async fn blocking_asset_handler_success() {
        let (participant, rx) = make_participant_with_rx("alice");
        let guard = participant.fetch_asset_sem().try_acquire().unwrap();
        let responder = AssetResponder::new(test_client(&participant), 7, guard);

        let handler = BlockingAssetHandlerFn(Arc::new(
            |_client: Client, _uri: String| -> Result<&[u8], &str> { Ok(b"<robot/>") },
        ));
        handler.fetch("package://test/model.urdf".to_string(), responder);

        let msg = tokio::time::timeout(Duration::from_secs(1), rx.recv_async())
            .await
            .expect("timed out waiting for asset response")
            .expect("channel closed");
        assert_eq!(
            msg.data,
            encode_binary_message(&FetchAssetResponse::asset_data(7, &b"<robot/>"[..]))
        );
    }

    #[tokio::test]
    async fn blocking_asset_handler_error() {
        let (participant, rx) = make_participant_with_rx("alice");
        let guard = participant.fetch_asset_sem().try_acquire().unwrap();
        let responder = AssetResponder::new(test_client(&participant), 9, guard);

        let handler = BlockingAssetHandlerFn(Arc::new(
            |_client: Client, _uri: String| -> Result<&[u8], &str> { Err("not found") },
        ));
        handler.fetch("package://missing".to_string(), responder);

        let msg = tokio::time::timeout(Duration::from_secs(1), rx.recv_async())
            .await
            .expect("timed out waiting for asset response")
            .expect("channel closed");
        assert_eq!(
            msg.data,
            encode_binary_message(&FetchAssetResponse::error_message(9, "not found"))
        );
    }

    #[tokio::test]
    async fn async_asset_handler_success() {
        let (participant, rx) = make_participant_with_rx("alice");
        let guard = participant.fetch_asset_sem().try_acquire().unwrap();
        let responder = AssetResponder::new(test_client(&participant), 8, guard);

        let handler = AsyncAssetHandlerFn(Arc::new(|_client: Client, _uri: String| async move {
            Ok::<_, String>(b"PNG data".to_vec())
        }));
        handler.fetch("https://example.com/asset.png".to_string(), responder);

        let msg = tokio::time::timeout(Duration::from_secs(1), rx.recv_async())
            .await
            .expect("timed out waiting for asset response")
            .expect("channel closed");
        assert_eq!(
            msg.data,
            encode_binary_message(&FetchAssetResponse::asset_data(8, &b"PNG data"[..]))
        );
    }
}

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt;
use libwebrtc::video_source::{RtcVideoSource, native::NativeVideoSource};
use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::{ByteStreamReader, Room, StreamByteOptions, id::ParticipantIdentity};
use parking_lot::RwLock;
use smallvec::SmallVec;
use tokio::io::AsyncReadExt;
use tokio_util::{io::StreamReader, sync::CancellationToken};
use tracing::{debug, error, info, warn};

use crate::protocol::v2::DecodeError;
use crate::remote_access::participant::ChannelWriter;
use crate::{
    ChannelDescriptor, ChannelId, Context, FoxgloveError, Metadata, RawChannel, Schema, Sink,
    SinkChannelFilter, SinkId,
    protocol::v2::{
        BinaryMessage, JsonMessage,
        client::{self, ClientMessage},
        server::{MessageData as ServerMessageData, ServerInfo, Status, Unadvertise, advertise},
    },
    remote_access::{
        Capability, Listener, RemoteAccessError, client::Client, participant::Participant,
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

pub(crate) const WS_PROTOCOL_TOPIC: &str = "ws-protocol";
pub(crate) const CLIENT_CHANNEL_TOPIC_PREFIX: &str = "client-";
const CHANNEL_TOPIC_PREFIX: &str = "ch-";
const MESSAGE_FRAME_SIZE: usize = 5; // 1 byte opcode + u32 LE length
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16 MiB
const MAX_SEND_RETRIES: usize = 3;
pub(crate) const DEFAULT_PENDING_CLIENT_READER_TIMEOUT: Duration = Duration::from_secs(15);

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

/// Frames a text payload with the v2 message framing (1 byte opcode + 4 byte LE length + payload).
fn frame_text_message(payload: &[u8]) -> Bytes {
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
    state: RwLock<SessionState>,
    channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    listener: Option<Arc<dyn Listener>>,
    capabilities: Vec<Capability>,
    cancellation_token: CancellationToken,
    data_plane_tx: flume::Sender<ChannelMessage>,
    data_plane_rx: flume::Receiver<ChannelMessage>,
    control_plane_tx: flume::Sender<ControlPlaneMessage>,
    control_plane_rx: flume::Receiver<ControlPlaneMessage>,
    /// Serializes all participant-scoped state mutations: subscription changes, video track
    /// lifecycle operations, client channel advertise/unadvertise, and participant removal.
    /// This prevents TOCTOU races between byte-stream message handlers and room-event handlers,
    /// which run on separate tokio tasks.
    subscription_lock: parking_lot::Mutex<()>,
    /// Signaled by video publishers when video metadata changes, prompting
    /// the sender loop to re-advertise affected channels.
    video_metadata_tx: tokio::sync::watch::Sender<()>,
    video_metadata_rx: tokio::sync::watch::Receiver<()>,
    /// Byte stream readers for `client-{channelId}` streams that arrived before the
    /// corresponding Client Advertise message. Keyed by participant identity then channel ID.
    /// Drained when the advertise arrives; expired after `pending_client_reader_timeout`.
    pending_client_readers:
        parking_lot::Mutex<HashMap<ParticipantIdentity, HashMap<ChannelId, ByteStreamReader>>>,
    pending_client_reader_timeout: Duration,
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
            let data = encode_binary_message(&message);
            self.send_data_lossy(ChannelMessage { channel_id, data });
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

        let framed = frame_text_message(advertise_msg.to_string().as_bytes());
        self.broadcast_control(framed);

        // Clients subscribe asynchronously.
        None
    }

    fn remove_channel(&self, channel: &RawChannel) {
        let _guard = self.subscription_lock.lock();
        let channel_id = channel.id();
        if !self.state.write().remove_channel(channel_id) {
            return;
        }

        self.teardown_video_track(channel_id);
        self.state.write().remove_video_schema(&channel_id);

        let unadvertise = Unadvertise::new([u64::from(channel_id)]);
        let framed = frame_text_message(unadvertise.to_string().as_bytes());
        self.broadcast_control(framed);
    }

    fn auto_subscribe(&self) -> bool {
        false
    }
}

impl RemoteAccessSession {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        room: Room,
        context: Weak<Context>,
        channel_filter: Option<Arc<dyn SinkChannelFilter>>,
        listener: Option<Arc<dyn Listener>>,
        capabilities: Vec<Capability>,
        cancellation_token: CancellationToken,
        message_backlog_size: usize,
        pending_client_reader_timeout: Duration,
    ) -> Self {
        let (data_plane_tx, data_plane_rx) = flume::bounded(message_backlog_size);
        let (control_plane_tx, control_plane_rx) = flume::bounded(message_backlog_size);
        let (video_metadata_tx, video_metadata_rx) = tokio::sync::watch::channel(());
        Self {
            sink_id: SinkId::next(),
            room,
            context,
            state: RwLock::new(SessionState::new()),
            channel_filter,
            listener,
            capabilities,
            cancellation_token,
            data_plane_tx,
            data_plane_rx,
            control_plane_tx,
            control_plane_rx,
            subscription_lock: parking_lot::Mutex::new(()),
            video_metadata_tx,
            video_metadata_rx,
            pending_client_readers: parking_lot::Mutex::new(HashMap::new()),
            pending_client_reader_timeout,
        }
    }

    /// Returns true if the given capability is enabled for this session.
    fn has_capability(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
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
        let framed = frame_text_message(status.to_string().as_bytes());
        self.send_control(participant.clone(), framed);
    }

    /// Send a warning status message to a participant.
    fn send_warning(&self, participant: &Arc<Participant>, message: String) {
        debug!("Sending warning to {participant}: {message}");
        let status = Status::warning(message);
        let framed = frame_text_message(status.to_string().as_bytes());
        self.send_control(participant.clone(), framed);
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
    /// `expected_channel_id` identifies a `client-{channelId}` stream: the channel ID parsed from
    /// the topic name. Every `MessageData` frame on this stream must carry the same channel ID;
    /// mismatches are considered a protocol violation (debug-asserted). Pass `None` for the
    /// `ws-protocol` control stream.
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
                self.handle_channel_stream_message(
                    &participant_identity,
                    channel_id,
                    opcode,
                    &payload,
                );
            } else if !self.handle_client_message(
                &participant_identity,
                opcode,
                Bytes::from(payload),
            ) {
                return;
            }
        }
    }

    /// Handle an incoming `client-{channelId}` byte stream.
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

        pending
            .entry(participant_identity.clone())
            .or_default()
            .insert(channel_id, reader);
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

    /// Handle a single framed ws-protocol message. Returns `false` if the byte stream
    /// should be closed (e.g. unrecognized opcode indicating a protocol mismatch).
    fn handle_client_message(
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
                    "Received MessageData over ws-protocol; MessageData is only supported on channel-specific byte streams"
                );
            }
            _ => {
                warn!("Unhandled client message: {client_msg:?}");
            }
        }
        true
    }

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
        let first_subscribed = state.subscribe(participant, &channel_ids);
        state.subscribe_data(participant, &data_channel_ids);
        state.unsubscribe_data(participant, &video_channel_ids);
        let first_video_subscribed = state.subscribe_video(participant, &video_channel_ids);
        let last_video_unsubscribed = state.unsubscribe_video(participant, &data_channel_ids);
        drop(state);

        if !first_subscribed.is_empty() {
            if let Some(context) = self.context.upgrade() {
                context.subscribe_channels(self.sink_id, &first_subscribed);
            }
        }

        self.start_video_tracks(&first_video_subscribed);
        self.stop_video_tracks(&last_video_unsubscribed);
    }

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
        let last_unsubscribed = state.unsubscribe(participant, &channel_ids);
        state.unsubscribe_data(participant, &channel_ids);
        let last_video_unsubscribed = state.unsubscribe_video(participant, &channel_ids);
        drop(state);

        if !last_unsubscribed.is_empty() {
            if let Some(context) = self.context.upgrade() {
                context.unsubscribe_channels(self.sink_id, &last_unsubscribed);
            }
        }

        self.stop_video_tracks(&last_video_unsubscribed);
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

            // Decode the schema, tolerating absent schemas.
            let schema = match ch.decode_schema() {
                Ok(data) => Some(Schema {
                    name: ch.schema_name.to_string(),
                    encoding: ch.schema_encoding.as_deref().unwrap_or("").to_string(),
                    data: data.into(),
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
                listener.on_client_advertise(client.clone(), &descriptor);
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
                None => {
                    self.send_warning(
                        participant,
                        format!(
                            "Client is not advertising channel: {channel_id_raw}; ignoring unadvertisement"
                        ),
                    );
                }
                Some(descriptor) => {
                    if let Some(listener) = &self.listener {
                        listener.on_client_unadvertise(client.clone(), &descriptor);
                    }
                }
            }
        }
    }

    /// Handle a message from a `client-{channelId}` byte stream.
    ///
    /// Only `MessageData` frames are expected. `expected_channel_id` is the channel ID parsed from
    /// the stream topic and must match the `channel_id` field inside every `MessageData` frame.
    /// A mismatch indicates a misbehaving client (the topic determines which stream carries the
    /// data, but the channel ID inside the message determines which descriptor is used).
    fn handle_channel_stream_message(
        self: &Arc<Self>,
        participant_identity: &ParticipantIdentity,
        expected_channel_id: u32,
        opcode: u8,
        payload: &[u8],
    ) {
        const BINARY: u8 = OpCode::Binary as u8;
        if opcode != BINARY {
            error!("Unexpected non-binary message on channel stream (opcode {opcode})");
            return;
        }
        let msg = match ClientMessage::parse_binary(payload) {
            Ok(ClientMessage::MessageData(msg)) => msg,
            Ok(other) => {
                error!(
                    "Unexpected message on channel stream: {other:?}; only MessageData is supported"
                );
                return;
            }
            Err(e) => {
                error!("Failed to parse channel stream message: {e:?}");
                return;
            }
        };
        assert_eq!(
            expected_channel_id, msg.channel_id,
            "MessageData channel_id ({}) does not match the stream topic channel_id ({})",
            msg.channel_id, expected_channel_id,
        );
        let Some(participant) = self.state.read().get_participant(participant_identity) else {
            error!("Unknown participant identity: {participant_identity:?}");
            return;
        };
        self.handle_client_message_data(&participant, msg);
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
            listener.on_message_data(client, &descriptor, &msg.data);
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
        server_info: ServerInfo,
    ) -> Result<(), Box<RemoteAccessError>> {
        use crate::remote_access::participant::ParticipantWriter;

        if self.state.read().has_participant(&participant_id) {
            return Ok(());
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

        // Send initial messages prior to adding the participant to the state map, to ensure that
        // these are the first messages delivered to the participant. This is safe to do without
        // holding the write lock, because this is a new participant - see below.
        info!("sending server info and advertisements to participant {participant:?}");
        let server_info_msg = frame_text_message(server_info.to_string().as_bytes());
        self.send_control(participant.clone(), server_info_msg);
        self.send_channel_advertisements(participant.clone());

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

        if !removed.client_channels.is_empty() {
            if let Some((listener, client_id)) = self.listener.as_ref().zip(removed.client_id) {
                let client = Client::new(client_id, participant_id.clone());
                for descriptor in &removed.client_channels {
                    listener.on_client_unadvertise(client.clone(), descriptor);
                }
            }
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

        let framed = frame_text_message(advertise_msg.to_string().as_bytes());
        self.send_control(participant, framed);
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
            let framed = frame_text_message(msg.to_string().as_bytes());
            self.broadcast_control(framed);
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
    use crate::remote_access::participant::{
        ParticipantWriter, TestByteStreamWriter, TestChannelWriter,
    };

    fn make_participant(name: &str) -> (ParticipantIdentity, Arc<Participant>) {
        let identity = ParticipantIdentity(name.to_string());
        let writer = Arc::new(TestByteStreamWriter::default());
        let participant = Arc::new(Participant::new(
            identity.clone(),
            ParticipantWriter::Test(writer),
        ));
        (identity, participant)
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
}

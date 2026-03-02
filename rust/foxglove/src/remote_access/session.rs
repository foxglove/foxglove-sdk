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

use crate::{
    ChannelId, Context, FoxgloveError, Metadata, RawChannel, Sink, SinkChannelFilter, SinkId,
    protocol::v2::{
        BinaryMessage, JsonMessage,
        client::{self, ClientMessage},
        server::{MessageData as ServerMessageData, ServerInfo, Unadvertise, advertise},
    },
    remote_access::{
        RemoteAccessError,
        participant::{Participant, ParticipantWriter},
        session_state::SessionState,
    },
};

mod video_track;
pub(crate) use video_track::{VideoInputSchema, VideoPublisher, get_video_input_schema};

const WS_PROTOCOL_TOPIC: &str = "ws-protocol";
const MESSAGE_FRAME_SIZE: usize = 5; // 1 byte opcode + u32 LE length
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16 MiB
const MAX_SEND_RETRIES: usize = 3;

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
    cancellation_token: CancellationToken,
    data_plane_tx: flume::Sender<ChannelMessage>,
    data_plane_rx: flume::Receiver<ChannelMessage>,
    control_plane_tx: flume::Sender<ControlPlaneMessage>,
    control_plane_rx: flume::Receiver<ControlPlaneMessage>,
    /// Serializes subscription changes and their associated video track lifecycle
    /// operations, which must not interleave across participants.
    subscription_lock: parking_lot::Mutex<()>,
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

        // If a video publisher is active for this channel, send the frame to it
        // and skip the raw data plane.
        if let Some(publisher) = self.state.read().get_video_publisher(&channel_id) {
            publisher.send(Bytes::copy_from_slice(msg), metadata.log_time);
            return Ok(());
        }

        let message = ServerMessageData::new(u64::from(channel_id), metadata.log_time, msg);
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
            state.inject_video_track_metadata(&mut advertise_msg);
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
    pub(crate) fn new(
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
            state: RwLock::new(SessionState::new()),
            channel_filter,
            cancellation_token,
            data_plane_tx,
            data_plane_rx,
            control_plane_tx,
            control_plane_rx,
            subscription_lock: parking_lot::Mutex::new(()),
        }
    }

    pub(crate) fn sink_id(&self) -> SinkId {
        self.sink_id
    }

    pub(crate) fn room(&self) -> &Room {
        &self.room
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
        let participants = self.state.read().collect_participants();
        for participant in participants {
            self.send_control(participant, data.clone());
        }
    }

    /// Reads from the data plane and control plane queues and sends messages to participants.
    ///
    /// Control plane messages are sent to the targeted participant.
    /// Data plane messages are sent to all participants subscribed to the message's channel.
    pub(crate) async fn run_sender(session: Arc<Self>) {
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
                    let participants: SmallVec<[Arc<Participant>; 8]> = {
                        let state = session.state.read();
                        let Some(ids) = state.collect_subscribers(&msg.channel_id) else {
                            continue;
                        };
                        ids.iter().filter_map(|id| state.get_participant(id)).collect()
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

    pub(crate) async fn handle_byte_stream_from_client(
        self: &Arc<Self>,
        participant_identity: ParticipantIdentity,
        reader: ByteStreamReader,
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

            if !self.handle_client_message(&participant_identity, opcode, Bytes::from(payload)) {
                return;
            }
        }
    }

    /// Handle a single framed client message. Returns `false` if the byte stream
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
            // TODO: Implement other message handling branches
            _ => {
                warn!("Unhandled client message: {client_msg:?}");
            }
        }
        true
    }

    fn handle_client_subscribe(
        self: &Arc<Self>,
        participant: &Participant,
        msg: client::Subscribe,
    ) {
        let _guard = self.subscription_lock.lock();
        let channel_ids: Vec<ChannelId> = msg
            .channel_ids
            .iter()
            .map(|&id| ChannelId::new(id))
            .collect();

        let first_subscribed = self.state.write().subscribe(participant, &channel_ids);

        if !first_subscribed.is_empty() {
            if let Some(context) = self.context.upgrade() {
                context.subscribe_channels(self.sink_id, &first_subscribed);
            }
        }

        self.start_video_tracks(&first_subscribed);
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

        let last_unsubscribed = self.state.write().unsubscribe(participant, &channel_ids);

        if !last_unsubscribed.is_empty() {
            if let Some(context) = self.context.upgrade() {
                context.unsubscribe_channels(self.sink_id, &last_unsubscribed);
            }
        }

        self.stop_video_tracks(&last_unsubscribed);
    }

    /// Add a participant to the server, if it hasn't already been added.
    /// In either case, return the new or existing participant.
    pub(crate) async fn add_participant(
        &self,
        participant_id: ParticipantIdentity,
    ) -> Result<Arc<Participant>, RemoteAccessError> {
        if let Some(existing) = self.state.read().get_participant(&participant_id) {
            return Ok(existing);
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

        Ok(self
            .state
            .write()
            .insert_participant(participant_id, participant))
    }

    /// Remove a participant from the session, cleaning up its subscriptions.
    ///
    /// Channels that lose their last subscriber are unsubscribed from the context.
    pub(crate) fn remove_participant(self: &Arc<Self>, participant_id: &ParticipantIdentity) {
        let _guard = self.subscription_lock.lock();
        let last_unsubscribed = self.state.write().remove_participant(participant_id);

        if !last_unsubscribed.is_empty() {
            if let Some(context) = self.context.upgrade() {
                context.unsubscribe_channels(self.sink_id, &last_unsubscribed);
            }
        }

        self.stop_video_tracks(&last_unsubscribed);
    }

    /// Enqueue server info and channel advertisements for delivery to a participant.
    ///
    /// Messages are routed through the control plane queue so that all writes to the
    /// participant's ByteStreamWriter are serialized by the sender task. ByteStreamWriter::write
    /// is not safe to call concurrently from multiple tasks.
    pub(crate) fn send_info_and_advertisements(
        &self,
        participant: Arc<Participant>,
        server_info: ServerInfo,
    ) {
        info!("sending server info and advertisements to participant {participant:?}");
        let framed = frame_text_message(server_info.to_string().as_bytes());
        self.send_control(participant.clone(), framed);
        self.send_channel_advertisements(participant);
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
                    state.inject_video_track_metadata(&mut msg);
                    Some(msg)
                })
                .flatten()
        }) else {
            return;
        };

        let framed = frame_text_message(advertise_msg.to_string().as_bytes());
        self.send_control(participant, framed);
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
            let publisher = Arc::new(VideoPublisher::new(video_source.clone(), input_schema));
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
    /// track. Does not remove the video schema, which persists for the lifetime of the channel.
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

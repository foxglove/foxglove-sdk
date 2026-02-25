use std::sync::{Arc, Weak};
use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt;
use livekit::{ByteStreamReader, Room, StreamByteOptions, id::ParticipantIdentity};
use parking_lot::RwLock;
use smallvec::SmallVec;
use tokio::io::AsyncReadExt;
use tokio_util::{io::StreamReader, sync::CancellationToken};
use tracing::{error, info, warn};

use crate::{
    ChannelId, Context, FoxgloveError, Metadata, RawChannel, Sink, SinkChannelFilter, SinkId,
    protocol::v2::{
        BinaryMessage, JsonMessage,
        client::{self, ClientMessage},
        server::{MessageData as ServerMessageData, ServerInfo, Unadvertise, advertise},
    },
    remote_access::{RemoteAccessError, participant::Participant, session_state::SessionState},
};

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

        let advertise_msg = advertise::advertise_channels(filtered.iter().copied());
        if advertise_msg.channels.is_empty() {
            return None;
        }

        // Track advertised channels, excluding any that failed to encode (e.g. MissingSchema).
        let advertised_ids: std::collections::HashSet<u64> =
            advertise_msg.channels.iter().map(|ch| ch.id).collect();
        let mut state = self.state.write();
        for &ch in &filtered {
            if advertised_ids.contains(&u64::from(ch.id())) {
                state.insert_channel(ch);
            }
        }
        drop(state);

        let framed = frame_text_message(advertise_msg.to_string().as_bytes());
        self.broadcast_control(framed);

        // Clients subscribe asynchronously.
        None
    }

    fn remove_channel(&self, channel: &RawChannel) {
        let channel_id = channel.id();
        if !self.state.write().remove_channel(channel_id) {
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

    fn handle_client_subscribe(&self, participant: &Participant, msg: client::Subscribe) {
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
    }

    fn handle_client_unsubscribe(&self, participant: &Participant, msg: client::Unsubscribe) {
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
    }

    /// Add a participant to the server, if it hasn't already been added.
    /// In either case, return the new or existing participant.
    pub(crate) async fn add_participant(
        &self,
        participant_id: ParticipantIdentity,
    ) -> Result<Arc<Participant>, RemoteAccessError> {
        use crate::remote_access::participant::ParticipantWriter;

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
    pub(crate) fn remove_participant(&self, participant_id: &ParticipantIdentity) {
        let last_unsubscribed = self.state.write().remove_participant(participant_id);

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
        let Some(framed) = self
            .state
            .read()
            .with_channels(|channels| {
                let advertise_msg = advertise::advertise_channels(channels.values());
                if advertise_msg.channels.is_empty() {
                    return None;
                }
                Some(frame_text_message(advertise_msg.to_string().as_bytes()))
            })
            .flatten()
        else {
            return;
        };

        self.send_control(participant, framed);
    }
}

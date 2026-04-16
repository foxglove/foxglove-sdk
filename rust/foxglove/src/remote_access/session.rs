use std::collections::HashMap;
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
use tokio::runtime::Handle;
use tokio_util::{io::StreamReader, sync::CancellationToken};
use tracing::{debug, error, info, trace, warn};

use crate::protocol::v2::DecodeError;
use crate::protocol::v2::parameter::Parameter;
use crate::protocol::v2::server::ParameterValues;
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
            AdvertiseServices, Pong, RemoveStatus, ServerInfo, ServiceCallFailure, Status,
            Unadvertise, UnadvertiseServices, advertise, advertise_services,
        },
    },
    remote_access::{
        AssetHandler, Capability, Listener, RemoteAccessError, client::Client,
        participant::Participant, protocol_version, rtt_tracker::RttTracker,
        session_state::SessionState,
    },
};

mod data_track;
pub(crate) use data_track::DataTrack;
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
const MESSAGE_FRAME_SIZE: usize = 5; // 1 byte opcode + u32 LE length
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16 MiB

/// Default size of the per-participant control plane queue. Large enough to buffer the
/// initial burst from `add_participant` (server info + channel/service advertisements)
/// without blocking, small enough to detect a slow participant via `try_send`.
pub(crate) const DEFAULT_CONTROL_QUEUE_SIZE: usize = 256;

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
pub(super) fn encode_json_message(message: &impl JsonMessage) -> Bytes {
    let payload = message.to_string();
    let payload = payload.as_bytes();
    let mut buf = Vec::with_capacity(MESSAGE_FRAME_SIZE + payload.len());
    buf.push(OpCode::Text as u8);
    let len = u32::try_from(payload.len()).expect("message too large");
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(payload);
    Bytes::from(buf)
}

pub(super) fn encode_binary_message<'a>(message: &impl BinaryMessage<'a>) -> Bytes {
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
    runtime: Handle,
    cancellation_token: CancellationToken,
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
    rtt_tracker: parking_lot::Mutex<RttTracker>,
    ice_rtt_tracker: parking_lot::Mutex<RttTracker>,
    connection_graph: Arc<parking_lot::Mutex<ConnectionGraph>>,
    /// Immutable `ServerInfo` message sent to each participant on connect and reset.
    server_info: ServerInfo,
    /// Channel used by per-participant flush tasks to request participant resets
    /// when a control stream write fails. `handle_room_events` receives identities
    /// and calls `reset_participant`. Using `mpsc` rather than `Notify` because
    /// `Notify` is not cancel-safe in `select!`.
    participant_reset_tx: tokio::sync::mpsc::UnboundedSender<ParticipantIdentity>,
    participant_reset_rx:
        tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<ParticipantIdentity>>,
    /// Size of the per-participant control plane queue.
    message_backlog_size: usize,
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

        // Send to data subscribers via the data track.
        if let Some(track) = state.get_subscribed_data_track(&channel_id) {
            track.log(channel_id, msg, metadata);
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
        let advertised_channel_ids: SmallVec<[ChannelId; 4]> = {
            let mut state = self.state.write();
            let mut ids = SmallVec::new();
            for &ch in &filtered {
                if advertised_ids.contains(&u64::from(ch.id())) {
                    state.insert_channel(ch);
                    ids.push(ch.id());
                    if let Some(input_schema) = get_video_input_schema(ch) {
                        state.insert_video_schema(ch.id(), input_schema);
                    }
                }
            }
            state.add_metadata_to_advertisement(&mut advertise_msg);
            ids
        };

        self.broadcast_control(encode_json_message(&advertise_msg));

        // Eagerly publish a data track for each newly advertised channel.
        self.publish_data_tracks(&advertised_channel_ids);

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
        self.teardown_data_track(channel_id);
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
    pub runtime: Handle,
    pub cancellation_token: CancellationToken,
    pub message_backlog_size: usize,
    pub services: Arc<parking_lot::RwLock<ServiceMap>>,
    pub connection_graph: Arc<parking_lot::Mutex<ConnectionGraph>>,
    pub remote_access_session_id: Option<String>,
    pub fetch_asset_handler: Option<Arc<dyn AssetHandler<Client>>>,
    pub server_info: ServerInfo,
}

impl RemoteAccessSession {
    pub(crate) fn new(params: SessionParams) -> Self {
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
            runtime: params.runtime,
            cancellation_token: params.cancellation_token,
            subscription_lock: parking_lot::Mutex::new(()),
            video_metadata_tx,
            video_metadata_rx,
            services: params.services,
            supported_encodings: params.supported_encodings,
            rtt_tracker: parking_lot::Mutex::new(RttTracker::new("ping/pong")),
            ice_rtt_tracker: parking_lot::Mutex::new(RttTracker::new("ICE")),
            connection_graph: params.connection_graph,
            server_info: params.server_info,
            participant_reset_tx,
            participant_reset_rx: tokio::sync::Mutex::new(participant_reset_rx),
            message_backlog_size: params.message_backlog_size,
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

    /// Send a control plane message to a participant. If the queue is full,
    /// the participant is disconnected (reset requested).
    fn send_control(&self, participant: &Participant, data: Bytes) {
        if !participant.try_queue_control(data) {
            let _ = self
                .participant_reset_tx
                .send(participant.participant_id().clone());
        }
    }

    /// Send an error status message to a participant.
    ///
    /// Best-effort: if the participant's queue is full, the message is dropped
    /// (the participant may already be disconnecting).
    fn send_error(&self, participant: &Participant, message: String) {
        debug!("Sending error to {participant}: {message}");
        let status = Status::error(message);
        let _ = participant.try_queue_control(encode_json_message(&status));
    }

    /// Send a warning status message to a participant.
    ///
    /// Best-effort: if the participant's queue is full, the message is dropped
    /// (the participant may already be disconnecting).
    fn send_warning(&self, participant: &Participant, message: String) {
        debug!("Sending warning to {participant}: {message}");
        let status = Status::warning(message);
        let _ = participant.try_queue_control(encode_json_message(&status));
    }

    /// Enqueue a control plane message for all currently connected participants.
    /// If a participant's queue is full, a reset is requested for that participant.
    fn broadcast_control(&self, data: Bytes) {
        let participants = self.state.read().collect_participants();
        for participant in participants {
            self.send_control(&participant, data.clone());
        }
    }

    /// Watches for video metadata changes and re-advertises affected channels.
    ///
    /// Runs until the cancellation token fires.
    pub(crate) async fn run_video_metadata_watcher(session: Arc<Self>) {
        let mut video_metadata: HashMap<ChannelId, VideoMetadata> = HashMap::new();
        let mut video_metadata_rx = session.video_metadata_rx.clone();
        loop {
            tokio::select! {
                biased;
                () = session.cancellation_token.cancelled() => break,
                Ok(()) = video_metadata_rx.changed() => {
                    session.republish_video_metadata(&mut video_metadata);
                }
            }
        }
    }

    /// Shut down the session: clear all participants (dropping their control
    /// queue senders so flush tasks exit), await the flush task handles, then
    /// close the LiveKit room.
    ///
    /// The caller must ensure that `handle_room_events` has stopped so no new
    /// `remove_participant` / `reset_participant` calls can race with us.
    pub(crate) async fn close(&self) {
        let flush_handles = {
            let mut state = self.state.write();
            // Clear participants first — this drops `Arc<Participant>` which drops
            // `control_tx`, causing each flush task's `recv_async` to return `Err`
            // and exit. Without this, flush tasks hang if the `CancellationToken`
            // hasn't been fired (e.g., on room disconnect without cancellation).
            state.clear_participants();
            state.take_flush_handles()
        };
        for handle in flush_handles {
            let _ = handle.await;
        }
        if let Err(e) = self.room.close().await {
            error!(
                remote_access_session_id = self.remote_access_session_id(),
                error = %e,
                "failed to close room: {e}",
            );
        }
    }

    /// Read framed messages from a client byte stream on the control channel.
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

            if !self.handle_client_control_message(
                &participant_identity,
                opcode,
                Bytes::from(payload),
            ) {
                return;
            }
        }
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
            ClientMessage::MessageData(msg) => {
                self.handle_client_message_data(&participant, msg);
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
                self.send_control(&participant, framed);
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

        // Create a per-participant control plane channel and spawn a flush task
        // that drains it into the byte stream writer.
        let (control_tx, control_rx) = flume::bounded::<Bytes>(self.message_backlog_size);
        let writer = ParticipantWriter::Livekit(stream);
        let participant_id_for_task = participant_id.clone();
        let reset_tx = self.participant_reset_tx.clone();
        let cancel = self.cancellation_token.clone();

        let flush_handle = tokio::spawn(async move {
            loop {
                let data = tokio::select! {
                    () = cancel.cancelled() => break,
                    msg = control_rx.recv_async() => match msg {
                        Ok(data) => data,
                        Err(_) => break,
                    },
                };
                if let Err(e) = writer.write(&data).await {
                    warn!(
                        "control write failed for {:?}, requesting reset: {e:?}",
                        participant_id_for_task,
                    );
                    let _ = reset_tx.send(participant_id_for_task.clone());
                    break;
                }
            }
        });

        let participant = Arc::new(Participant::new(
            participant_id.clone(),
            protocol_version,
            control_tx,
        ));

        // Send initial messages prior to adding the participant to the state map, to ensure that
        // these are the first messages delivered to the participant. This is safe to do without
        // holding the write lock, because this is a new participant — see below.
        info!("sending server info and advertisements to participant {participant:?}");
        let _ = participant.try_queue_control(encode_json_message(&self.server_info));
        self.send_channel_advertisements(participant.clone());
        self.send_service_advertisements(participant.clone());

        // Add the participant to the state map. We assert that this is a new participant, because
        // we validated that it did not exist in the map at the top of this function, and the
        // caller is responsible for ensuring this function is not called concurrently for the same
        // participant identity.
        let mut state = self.state.write();
        let did_insert = state.insert_participant(participant_id.clone(), participant);
        assert!(did_insert);
        state.insert_flush_handle(participant_id, flush_handle);
        Ok(())
    }

    /// Remove a participant from the session, cleaning up its subscriptions.
    ///
    /// Channels that lose their last subscriber are unsubscribed from the context.
    pub(crate) fn remove_participant(self: &Arc<Self>, participant_id: &ParticipantIdentity) {
        let _guard = self.subscription_lock.lock();

        let removed = {
            let mut state = self.state.write();
            let removed = state.remove_participant(participant_id);
            // Detach the flush task — it exits when control_tx drops.
            drop(state.remove_flush_handle(participant_id));
            removed
        };

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
                                .handle_byte_stream_from_client(participant_identity, reader)
                                .await;
                        });
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
                trace!(remote_access_session_id, "room event: {:?}", event);
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

        let _ = participant.try_queue_control(encode_json_message(&advertise_msg));
    }

    /// Enqueue service advertisements for delivery to a single participant.
    fn send_service_advertisements(&self, participant: Arc<Participant>) {
        let services: Vec<_> = self.services.read().values().cloned().collect();
        if let Some(msg) = build_advertise_services_msg(&services) {
            let _ = participant.try_queue_control(encode_json_message(&msg));
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

        let responder =
            super::service::new_responder(participant, service_id, call_id, encoding, guard);
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
        self.send_control(participant, encode_json_message(&failure));
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
        self.send_control(participant, encode_json_message(&msg));
    }

    /// Publish parameter values to all participants subscribed to those parameters.
    pub(crate) fn publish_parameter_values(&self, parameters: Vec<Parameter>) {
        if !self.has_capability(Capability::Parameters) {
            error!("Server does not support parameters capability");
            return;
        }

        // Collect the per-participant messages while holding the read lock, then
        // send them after the lock is released to minimize lock scope.
        let to_send: Vec<(Arc<Participant>, Bytes)> = {
            let state = self.state.read();
            let participants = state.collect_participants();
            participants
                .into_iter()
                .filter_map(|participant| {
                    let filtered: Vec<_> = parameters
                        .iter()
                        .filter(|p| {
                            state
                                .parameter_subscribers(&p.name)
                                .is_some_and(|ids| ids.contains(participant.participant_id()))
                        })
                        .cloned()
                        .collect();

                    if filtered.is_empty() {
                        return None;
                    }

                    let msg =
                        ParameterValues::new(filtered.into_iter().filter(|p| p.value.is_some()));
                    Some((participant, encode_json_message(&msg)))
                })
                .collect()
        };

        for (participant, data) in to_send {
            self.send_control(&participant, data);
        }
    }

    /// Publish a status message to all connected participants.
    pub(crate) fn publish_status(&self, status: Status) {
        self.broadcast_control(encode_json_message(&status));
    }

    /// Remove status messages by ID from all connected participants.
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

        let encoded = {
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

            encode_json_message(&graph.as_initial_update())
        };

        self.send_control(participant, encoded);
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
                self.send_control(&participant, encoded.clone());
            }
        }
    }

    /// Check video publishers for metadata changes and re-advertise affected channels.
    ///
    /// Called from `run_video_metadata_watcher` when `video_metadata_rx` signals a change. Compares each
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

    /// Eagerly publish data tracks for newly advertised channels.
    fn publish_data_tracks(&self, topics: &[ChannelId]) {
        for channel_id in topics {
            let data_track = DataTrack::publish(
                &self.runtime,
                self.room.local_participant(),
                *channel_id,
                self.cancellation_token.clone(),
            );
            self.state
                .write()
                .insert_data_track(*channel_id, data_track);
        }
    }

    /// Tear down the data track for a channel.
    fn teardown_data_track(&self, channel_id: ChannelId) {
        if let Some(mut data_track) = self.state.write().remove_data_track(&channel_id) {
            self.runtime.spawn(async move { data_track.close().await });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::v2::server::FetchAssetResponse;
    use crate::remote_common::fetch_asset::{
        AssetHandler, AsyncAssetHandlerFn, BlockingAssetHandlerFn,
    };

    fn make_participant_with_rx(name: &str) -> (Arc<Participant>, flume::Receiver<Bytes>) {
        let identity = ParticipantIdentity(name.to_string());
        let version = protocol_version::REMOTE_ACCESS_PROTOCOL_VERSION.clone();
        let (tx, rx) = flume::bounded(16);
        let participant = Arc::new(Participant::new(identity, version, tx));
        (participant, rx)
    }

    fn test_client(participant: &Arc<Participant>) -> Client {
        Client::with_sender(
            participant.client_id(),
            participant.participant_id().clone(),
            participant,
        )
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
            msg,
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
            msg,
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
            msg,
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
            msg,
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
            msg,
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
            msg,
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
            msg,
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
            msg,
            encode_binary_message(&FetchAssetResponse::asset_data(8, &b"PNG data"[..]))
        );
    }

    // ---- flush task tests ----

    /// Spawns a flush task identical to the one in `add_participant`, using a test writer.
    /// Returns the control channel sender, the test writer (for inspecting writes), and
    /// the task's `JoinHandle`.
    fn spawn_test_flush_task(
        cancel: CancellationToken,
    ) -> (
        flume::Sender<Bytes>,
        Arc<crate::remote_access::participant::TestByteStreamWriter>,
        tokio::task::JoinHandle<()>,
    ) {
        use crate::remote_access::participant::{ParticipantWriter, TestByteStreamWriter};

        let (control_tx, control_rx) = flume::bounded::<Bytes>(DEFAULT_CONTROL_QUEUE_SIZE);
        let writer = Arc::new(TestByteStreamWriter::default());
        let writer_for_task = writer.clone();
        let handle = tokio::spawn(async move {
            let writer = ParticipantWriter::Test(writer_for_task);
            loop {
                let data = tokio::select! {
                    () = cancel.cancelled() => break,
                    msg = control_rx.recv_async() => match msg {
                        Ok(data) => data,
                        Err(_) => break,
                    },
                };
                if let Err(e) = writer.write(&data).await {
                    warn!("test flush task write failed: {e:?}");
                    break;
                }
            }
        });
        (control_tx, writer, handle)
    }

    #[tokio::test]
    async fn flush_task_delivers_messages() {
        let cancel = CancellationToken::new();
        let (tx, writer, handle) = spawn_test_flush_task(cancel.clone());

        tx.send(Bytes::from_static(b"hello")).unwrap();
        tx.send(Bytes::from_static(b"world")).unwrap();

        // Drop the sender to signal the flush task to exit.
        drop(tx);
        handle.await.unwrap();

        let writes = writer.writes();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0], Bytes::from_static(b"hello"));
        assert_eq!(writes[1], Bytes::from_static(b"world"));
    }

    #[tokio::test]
    async fn flush_task_stops_on_sender_drop() {
        let cancel = CancellationToken::new();
        let (tx, _writer, handle) = spawn_test_flush_task(cancel.clone());

        // Drop the sender without cancelling — task should exit because recv returns Err.
        drop(tx);

        let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(result.is_ok(), "flush task did not exit after sender drop");
    }

    #[tokio::test]
    async fn flush_task_stops_on_cancellation() {
        let cancel = CancellationToken::new();
        let (_tx, _writer, handle) = spawn_test_flush_task(cancel.clone());

        // Cancel without dropping the sender — task should exit via the select! arm.
        cancel.cancel();

        let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(result.is_ok(), "flush task did not exit after cancellation");
    }

    #[tokio::test]
    async fn flush_tasks_are_independent() {
        // Two flush tasks: task A is blocked by a Notify, task B should still make progress.
        let cancel = CancellationToken::new();
        let gate = Arc::new(tokio::sync::Notify::new());

        // Task A: uses a custom writer that blocks on the gate before completing.
        let (tx_a, control_rx_a) = flume::bounded::<Bytes>(DEFAULT_CONTROL_QUEUE_SIZE);
        let gate_for_a = gate.clone();
        let cancel_a = cancel.clone();
        let writer_a = Arc::new(crate::remote_access::participant::TestByteStreamWriter::default());
        let writer_a_for_task = writer_a.clone();
        let handle_a = tokio::spawn(async move {
            loop {
                let data = tokio::select! {
                    () = cancel_a.cancelled() => break,
                    msg = control_rx_a.recv_async() => match msg {
                        Ok(data) => data,
                        Err(_) => break,
                    },
                };
                // Wait for the gate before "writing".
                gate_for_a.notified().await;
                writer_a_for_task.record(&data);
            }
        });

        // Task B: normal flush task, no blocking.
        let (tx_b, writer_b, handle_b) = spawn_test_flush_task(cancel.clone());

        // Send a message to both.
        tx_a.send(Bytes::from_static(b"msg_a")).unwrap();
        tx_b.send(Bytes::from_static(b"msg_b")).unwrap();

        // Drop B's sender so it flushes and exits.
        drop(tx_b);
        let result = tokio::time::timeout(Duration::from_secs(1), handle_b).await;
        assert!(
            result.is_ok(),
            "task B should complete even though task A is blocked"
        );
        assert_eq!(writer_b.writes(), vec![Bytes::from_static(b"msg_b")]);

        // Task A hasn't written yet — still blocked on the gate.
        assert!(writer_a.writes().is_empty());

        // Release A.
        gate.notify_one();
        drop(tx_a);
        let result = tokio::time::timeout(Duration::from_secs(1), handle_a).await;
        assert!(result.is_ok(), "task A should complete after gate release");
        assert_eq!(writer_a.writes(), vec![Bytes::from_static(b"msg_a")]);
    }

    #[test]
    fn try_queue_control_returns_false_when_full() {
        let version = protocol_version::REMOTE_ACCESS_PROTOCOL_VERSION.clone();
        let (tx, _rx) = flume::bounded::<Bytes>(1);
        let participant = Participant::new(ParticipantIdentity("alice".to_string()), version, tx);

        // First message fits.
        assert!(participant.try_queue_control(Bytes::from_static(b"first")));
        // Second message overflows the 1-slot queue.
        assert!(!participant.try_queue_control(Bytes::from_static(b"second")));
    }

    #[test]
    fn try_queue_control_returns_true_when_disconnected() {
        let version = protocol_version::REMOTE_ACCESS_PROTOCOL_VERSION.clone();
        let (tx, rx) = flume::bounded::<Bytes>(1);
        let participant = Participant::new(ParticipantIdentity("alice".to_string()), version, tx);

        // Drop the receiver — channel disconnected.
        drop(rx);
        // Disconnected returns true (no reset needed).
        assert!(participant.try_queue_control(Bytes::from_static(b"msg")));
    }
}

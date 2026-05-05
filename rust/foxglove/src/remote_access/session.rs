use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt;
use indexmap::IndexSet;
use libwebrtc::video_source::{RtcVideoSource, native::NativeVideoSource};
use livekit::options::{TrackPublishOptions, VideoCodec};
use livekit::{
    ByteStreamReader, Room, StreamByteOptions,
    id::{ParticipantIdentity, ParticipantSid},
};
use livekit::{StreamWriter, prelude::*};
use parking_lot::RwLock;
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
            AdvertiseServices, MessageData, Pong, RemoveStatus, ServerInfo, ServiceCallFailure,
            Status, Unadvertise, UnadvertiseServices, advertise, advertise_services,
        },
    },
    remote_access::qos::{QosClassifier, Reliability},
    remote_access::{
        AssetHandler, Capability, Listener, RemoteAccessError,
        client::Client,
        participant::{Participant, ParticipantWriter},
        participant_registry::ParticipantRegistry,
        protocol_version,
        rtt_tracker::RttTracker,
        session_state::SessionState,
    },
};

mod data_track;
pub(super) use data_track::DataTrack;
mod video_track;
pub(super) use video_track::{
    VideoInputSchema, VideoMetadata, VideoPublisher, get_video_input_schema,
};

#[derive(Debug)]
struct SessionStats {
    participants: usize,
    subscriptions: usize,
    video_tracks: usize,
}

const CONTROL_CHANNEL_TOPIC: &str = "control";
const MESSAGE_FRAME_SIZE: usize = 5; // 1 byte opcode + u32 LE length
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024; // 16 MiB

/// Upper bound on `livekit::Room::close()`. The LiveKit SDK occasionally hangs
/// in its data-channel teardown path during room close, so we cap the wait to
/// keep `RemoteAccessGateway::stop()` (and the associated `ShuttingDown` ->
/// `Shutdown` status transition) bounded. Any remaining cleanup runs through
/// the `Room`'s `Drop` impl when the session is released.
const ROOM_CLOSE_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) const DEFAULT_MESSAGE_BACKLOG_SIZE: usize = 1024;

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
pub(super) struct RemoteAccessSession {
    sink_id: SinkId,
    room: Room,
    context: Weak<Context>,
    remote_access_session_id: Option<String>,
    /// Session-level state: channels, subscriptions, video publishers, client
    /// channels, parameter subscriptions. Participant membership lives on
    /// [`participant_registry`] instead.
    state: RwLock<SessionState>,
    channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    qos_classifier: Option<Arc<dyn QosClassifier>>,
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
    /// Participant membership and flush-task lifecycle.
    participant_registry: ParticipantRegistry,
    /// If set, how long the session may remain with zero active participants before returning
    /// to the dormant watch phase. Advertised by the API via the `hello` event's
    /// `deviceWaitForViewerMs` field.
    device_wait_for_viewer: Option<Duration>,
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

        // Collect subscriber identities under the session-state lock and
        // release it before broadcasting. Two races are possible between the
        // collect and the registry resolve, both benign:
        //   1. The participant has been removed and not replaced — the resolve
        //      misses and the message is dropped (their control queue has
        //      already been drained anyway).
        //   2. The participant has been removed and a same-identity reconnect
        //      has registered in their place. The resolve returns the new
        //      attempt, which receives a MessageData for a channel it never
        //      subscribed to. The channel ID is session-scoped (already
        //      advertised on this session), so the viewer drops the unknown
        //      subscription and continues. Resolution is keyed on
        //      ParticipantIdentity, so this can never deliver data across
        //      identities — only the same logical user across a reconnect.
        let reliable_subscribers = {
            let state = self.state.read();

            // Video track publisher: stays inside the state lock since the
            // publisher handle is not cloneable out of the map.
            if let Some(publisher) = state.get_video_publisher(&channel_id) {
                publisher.send(Bytes::copy_from_slice(msg), metadata.log_time);
            }

            if !state.has_data_subscribers(&channel_id) {
                None
            } else if state.qos_profile(&channel_id).reliability == Reliability::Reliable {
                Some(state.data_subscriber_identities(&channel_id))
            } else {
                // Lossy channels: send via the eagerly-published data track
                // inline, while we still hold the state read lock.
                if let Some(track) = state.get_subscribed_data_track(&channel_id) {
                    track.log(channel_id, msg, metadata);
                }
                None
            }
        };

        // Reliable channels: send MessageData via the control bytestream.
        // Batch-resolve identities so we take the registry lock once rather
        // than per-subscriber.
        if let Some(subscribers) = reliable_subscribers {
            let message = MessageData::new(u64::from(channel_id), metadata.log_time, msg);
            let encoded = encode_binary_message(&message);
            for participant in self.participant_registry.resolve_identities(subscribers) {
                participant.send_control(encoded.clone());
            }
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

        // Track advertised channels, detect video-capable ones, and classify QoS.
        let advertised_ids: std::collections::HashSet<u64> =
            advertise_msg.channels.iter().map(|ch| ch.id).collect();
        let advertised_channel_ids: SmallVec<[ChannelId; 4]> = {
            let mut state = self.state.write();
            let mut ids = SmallVec::new();
            for &ch in &filtered {
                if advertised_ids.contains(&u64::from(ch.id())) {
                    state.insert_channel(ch);
                    let video_schema = get_video_input_schema(ch);
                    if let Some(input_schema) = video_schema {
                        state.insert_video_schema(ch.id(), input_schema);
                    }
                    let mut qos = self
                        .qos_classifier
                        .as_ref()
                        .map(|c| c.classify(ch.descriptor()))
                        .unwrap_or_default();
                    if video_schema.is_some() && qos.reliability == Reliability::Reliable {
                        warn!(
                            "Forcing QoS to Lossy for video channel {:?} (topic={}): \
                             Reliable delivery is not supported for video",
                            ch.id(),
                            ch.topic()
                        );
                        qos.reliability = Reliability::Lossy;
                    }
                    state.insert_qos_profile(ch.id(), qos);
                    if qos.reliability != Reliability::Reliable {
                        ids.push(ch.id());
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

        // Collect subscriber identities before removal; we'll resolve them to
        // `Client`s after via the registry.
        let subscriber_identities = self.state.read().channel_subscriber_identities(&channel_id);

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
            for participant in self
                .participant_registry
                .resolve_identities(subscriber_identities)
            {
                let client = Client::new(
                    participant.client_id(),
                    participant.participant_id().clone(),
                );
                listener.on_unsubscribe(&client, descriptor);
            }
        }
    }

    fn auto_subscribe(&self) -> bool {
        false
    }
}

pub(super) struct SessionParams {
    pub(super) room: Room,
    pub(super) context: Weak<Context>,
    pub(super) channel_filter: Option<Arc<dyn SinkChannelFilter>>,
    pub(super) qos_classifier: Option<Arc<dyn QosClassifier>>,
    pub(super) listener: Option<Arc<dyn Listener>>,
    pub(super) capabilities: Vec<Capability>,
    pub(super) supported_encodings: IndexSet<String>,
    pub(super) runtime: Handle,
    pub(super) cancellation_token: CancellationToken,
    pub(super) message_backlog_size: usize,
    pub(super) services: Arc<parking_lot::RwLock<ServiceMap>>,
    pub(super) connection_graph: Arc<parking_lot::Mutex<ConnectionGraph>>,
    pub(super) remote_access_session_id: Option<String>,
    pub(super) fetch_asset_handler: Option<Arc<dyn AssetHandler<Client>>>,
    pub(super) server_info: ServerInfo,
    pub(super) device_wait_for_viewer: Option<Duration>,
}

impl RemoteAccessSession {
    pub(super) fn new(params: SessionParams) -> Self {
        let (video_metadata_tx, video_metadata_rx) = tokio::sync::watch::channel(());
        let participant_registry = ParticipantRegistry::new(params.message_backlog_size);
        Self {
            sink_id: SinkId::next(),
            room: params.room,
            context: params.context,
            remote_access_session_id: params.remote_access_session_id,
            state: RwLock::new(SessionState::new()),
            channel_filter: params.channel_filter,
            qos_classifier: params.qos_classifier,
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
            participant_registry,
            device_wait_for_viewer: params.device_wait_for_viewer,
        }
    }

    /// Returns true if the given capability is enabled for this session.
    fn has_capability(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }

    pub(super) fn remote_access_session_id(&self) -> Option<&str> {
        self.remote_access_session_id.as_deref()
    }

    pub(super) fn sink_id(&self) -> SinkId {
        self.sink_id
    }

    pub(super) fn room(&self) -> &Room {
        &self.room
    }

    fn stats(&self) -> SessionStats {
        let state = self.state.read();
        SessionStats {
            participants: self.participant_registry.participant_count(),
            subscriptions: state.subscription_count(),
            video_tracks: state.video_track_count(),
        }
    }

    /// Send an error status message to a participant.
    fn send_error(&self, participant: &Participant, message: String) {
        debug!("Sending error to {participant}: {message}");
        let status = Status::error(message);
        participant.send_control(encode_json_message(&status));
    }

    /// Send a warning status message to a participant.
    fn send_warning(&self, participant: &Participant, message: String) {
        debug!("Sending warning to {participant}: {message}");
        let status = Status::warning(message);
        participant.send_control(encode_json_message(&status));
    }

    /// Enqueue a control plane message for all currently connected participants.
    /// If a participant's queue is full, a reset is requested for that participant.
    fn broadcast_control(&self, data: Bytes) {
        for participant in self.participant_registry.collect_participants() {
            participant.send_control(data.clone());
        }
    }

    /// Watches for video metadata changes and re-advertises affected channels.
    ///
    /// Runs until the cancellation token fires.
    pub(super) async fn run_video_metadata_watcher(session: Arc<Self>) {
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

    /// Cancel the session's `CancellationToken`, signaling all session-scoped
    /// tasks to stop.
    pub(super) fn cancel(&self) {
        self.cancellation_token.cancel();
    }

    /// Shut down the session: cancel every participant's flush-task, await
    /// their completion, then close the LiveKit room.
    ///
    /// The caller must ensure that `handle_room_events` has stopped so no new
    /// `remove_participant` / `reset_participant` calls can race with us.
    pub(super) async fn close(self: &Arc<Self>) {
        // Cancel flush-tasks and await them before tearing down the transport.
        // In-flight writes either complete or fail once `room.close()` runs.
        self.participant_registry.shutdown().await;
        match tokio::time::timeout(ROOM_CLOSE_TIMEOUT, self.room.close()).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => error!(
                remote_access_session_id = self.remote_access_session_id(),
                error = %e,
                "failed to close room: {e}",
            ),
            Err(_) => {
                // LiveKit room.close() can hang indefinitely: #FLE-505
                // Our workaround here is to use a timeout and try again in the background.
                // The SFU should detect the device has left and kick it, but failing that
                // our monitoring code will detect the room has been idle with just the device
                // for too long, and shut it down.
                warn!(
                    remote_access_session_id = self.remote_access_session_id(),
                    timeout_secs = ROOM_CLOSE_TIMEOUT.as_secs(),
                    "livekit room close timed out; retrying in background",
                );
                let session = Arc::clone(self);
                tokio::spawn(async move {
                    match tokio::time::timeout(ROOM_CLOSE_TIMEOUT, session.room.close()).await {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => error!(
                            remote_access_session_id = session.remote_access_session_id(),
                            error = %e,
                            "failed to close room on retry: {e}",
                        ),
                        Err(_) => error!(
                            remote_access_session_id = session.remote_access_session_id(),
                            timeout_secs = ROOM_CLOSE_TIMEOUT.as_secs(),
                            "livekit room close timed out on retry; abandoning room teardown",
                        ),
                    }
                });
            }
        }
    }

    /// Read framed messages from a client byte stream on the control channel.
    pub(super) async fn handle_byte_stream_from_client(
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

        let Some(participant) = self
            .participant_registry
            .get_participant(participant_identity)
        else {
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
                participant.send_control(framed);
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
        let subscribe_result = state.subscribe(participant.participant_id(), &channel_ids);
        let first_video_subscribed =
            state.subscribe_video(participant.participant_id(), &video_channel_ids);
        let last_video_unsubscribed =
            state.unsubscribe_video(participant.participant_id(), &data_channel_ids);
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
        let unsubscribe_result = state.unsubscribe(participant.participant_id(), &channel_ids);
        let last_video_unsubscribed =
            state.unsubscribe_video(participant.participant_id(), &channel_ids);
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
    pub(super) async fn send_incompatible_version_error(
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
    /// `participant_sid` is the LiveKit session ID of the specific connection
    /// instance being registered; it's stored on the `Participant` so a later
    /// `ParticipantDisconnected` event (or a flush-task failure) can be matched
    /// against this instance rather than the identity alone.
    ///
    /// `joined_at` is the LiveKit-assigned join timestamp (ms since epoch)
    /// for this connection instance; it lets the registry reject a
    /// same-identity registration whose `joined_at` is older than the
    /// currently stored one (out-of-order `ParticipantActive` for a
    /// superseded instance).
    ///
    /// When a participant is added, a ServerInfo message and channel Advertisement messages are
    /// immediately queued for transmission.
    ///
    /// If a participant for `participant_id` is already registered with the
    /// **same** `participant_sid`, this is a no-op (the same connection instance
    /// is being re-announced — nothing to do). If the registered instance
    /// has a different SID but a `joined_at` that is **later** than
    /// `joined_at`, the incoming registration is also a no-op: it's a
    /// reordered `ParticipantActive` for an instance the server has
    /// already superseded. Otherwise this is treated as a same-identity
    /// reconnect: the new control stream is opened, the prior registration
    /// is atomically replaced, and the prior participant's cleanup runs
    /// (so its subscriptions are torn down and listener `on_unsubscribe` /
    /// `on_client_unadvertise` callbacks fire). This handles the case where
    /// LiveKit emits the reconnect's `ParticipantActive` *before* the
    /// prior instance's `ParticipantDisconnected`.
    pub(super) async fn add_participant(
        self: &Arc<Self>,
        participant_id: ParticipantIdentity,
        participant_sid: ParticipantSid,
        joined_at: i64,
    ) -> Result<(), Box<RemoteAccessError>> {
        // Gate on the registry *before* opening the stream: `stream_bytes`
        // is an RPC that should not be wasted on an already-registered
        // (identity, sid) pair, and equally not wasted on a stale incoming
        // instance the registry would reject. A different-SID hit with a
        // newer-or-equal `joined_at` means the registered instance is
        // older and we must fall through to open a stream for the new
        // instance.
        if let Some(existing) = self.participant_registry.get_participant(&participant_id) {
            if existing.participant_sid() == &participant_sid {
                return Ok(());
            }
            if existing.joined_at() > joined_at {
                info!(
                    remote_access_session_id = self.remote_access_session_id(),
                    participant_identity = %participant_id,
                    existing_sid = %existing.participant_sid(),
                    existing_joined_at = existing.joined_at(),
                    incoming_sid = %participant_sid,
                    incoming_joined_at = joined_at,
                    "skipping add_participant for stale instance (incoming joined_at precedes registered)",
                );
                return Ok(());
            }
        }

        let stream = self
            .room
            .local_participant()
            .stream_bytes(StreamByteOptions {
                topic: CONTROL_CHANNEL_TOPIC.to_string(),
                destination_identities: vec![participant_id.clone()],
                ..StreamByteOptions::default()
            })
            .await
            .inspect_err(|e| {
                error!("failed to open control stream for {participant_id}: {e:?}");
            })?;

        // Encode the initial messages (server info + channel / service
        // advertisements) up front. The registry queues them on the
        // participant's control-plane channel before inserting the
        // participant into state, so these are the first bytes the viewer
        // receives.
        let mut initial_messages = vec![encode_json_message(&self.server_info)];
        initial_messages.extend(self.encode_channel_advertisements());
        initial_messages.extend(self.encode_service_advertisements());

        info!(
            "registering participant {participant_id:?} with {} initial messages",
            initial_messages.len()
        );
        // Hold `subscription_lock` across the registry call + any cleanup
        // for a replaced prior, so a same-identity reconnect ordering is
        // serialized with concurrent subscribe / unsubscribe / remove paths.
        let _guard = self.subscription_lock.lock();
        let replaced = self.participant_registry.register_participant(
            participant_id.clone(),
            participant_sid.clone(),
            joined_at,
            ParticipantWriter::Livekit(stream),
            &self.cancellation_token,
            initial_messages,
        );
        if let Some(prior) = replaced {
            info!(
                remote_access_session_id = self.remote_access_session_id(),
                participant_identity = %participant_id,
                prior_sid = %prior.participant_sid(),
                new_sid = %participant_sid,
                "replaced same-identity participant on out-of-order ParticipantActive (new connection instance superseded the prior one)",
            );
            self.run_participant_removal_cleanup(&prior);
        }
        Ok(())
    }

    /// Removes the participant whose stored LiveKit SID matches `target_sid`,
    /// running the full cleanup (listener callbacks, context unsubscribe,
    /// video track teardown, connection-graph update) when removal happens.
    /// Returns the removed `Arc<Participant>` (so callers can capture the
    /// identity for re-registration), or `None` if no participant with this
    /// SID is registered.
    ///
    /// SID-keyed: a `ParticipantDisconnected` for a prior instance can arrive
    /// after a same-identity reconnect has replaced it, but the reconnected
    /// instance has a *different* SID, so a stale removal misses here rather
    /// than tearing down the replacement. Callers handle the `None` case
    /// according to their context.
    pub(super) fn remove_participant(
        self: &Arc<Self>,
        target_sid: &ParticipantSid,
    ) -> Option<Arc<Participant>> {
        let _guard = self.subscription_lock.lock();
        let participant = self.participant_registry.remove_participant(target_sid)?;
        self.run_participant_removal_cleanup(&participant);
        Some(participant)
    }

    /// Runs the post-removal cleanup for `participant`: subscription sweep,
    /// context unsubscribe, video-track teardown, connection-graph update,
    /// and listener callbacks.
    ///
    /// Caller must hold `subscription_lock` and have already removed
    /// `participant` from the registry.
    fn run_participant_removal_cleanup(self: &Arc<Self>, participant: &Arc<Participant>) {
        let client_id = participant.client_id();
        let participant_id = participant.participant_id();
        let removed = self
            .state
            .write()
            .cleanup_for_removed_identity(participant_id);

        // Listener / context / video-track / connection-graph aftercare.
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

        if self.has_capability(Capability::ConnectionGraph) {
            let mut graph = self.connection_graph.lock();
            if graph.remove_subscriber(client_id) && !graph.has_subscribers() {
                if let Some(listener) = &self.listener {
                    listener.on_connection_graph_unsubscribe();
                }
            }
        }

        if let Some(listener) = &self.listener {
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
    /// Returns when the room is disconnected, the event stream ends, or the session has been
    /// idle (no active participants) for longer than `device_wait_for_viewer`.
    pub(super) async fn handle_room_events(
        self: &Arc<Self>,
        mut room_events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    ) {
        let remote_access_session_id = self.remote_access_session_id();
        // Track when the room most recently had no active viewers. The idle countdown is
        // applied symmetrically to the initial join (in case a wake fires but the viewer
        // never arrives) and to the post-departure case ("after the last viewer leaves").
        // `device_wait_for_viewer` is sized large enough that the viewer has time to join
        // after a wake.
        let mut idle_since: Option<tokio::time::Instant> = None;
        loop {
            // Drain pending resets before waiting for events. This covers the case
            // where a `Notify::notified()` wakeup was lost due to `select!`
            // cancellation — the SIDs are still in the set even if the
            // notification was consumed by a dropped future.
            //
            // `handle_room_events` is the single task driving participant
            // membership during the session lifecycle, so the lookup inside
            // `reset_participant` cannot be invalidated before it runs. A
            // `ParticipantSid` no longer registered means the request is
            // stale — the participant was already removed and may have been
            // replaced by a reconnection that, by definition, has a different
            // SID; the staleness check inside `reset_participant` skips
            // those, avoiding a spurious teardown of the replacement.
            for sid in self.participant_registry.drain_pending_resets() {
                self.reset_participant(sid).await;
            }

            // Refresh the idle state based on current participant count.
            let active = self.participant_registry.participant_count();
            if active > 0 {
                idle_since = None;
            } else if idle_since.is_none() {
                idle_since = Some(tokio::time::Instant::now());
            }

            let idle_deadline = match (self.device_wait_for_viewer, idle_since) {
                (Some(wait), Some(since)) => Some(since + wait),
                _ => None,
            };

            tokio::select! {
                event = room_events.recv() => {
                    let Some(event) = event else { break };
                    if !self.handle_room_event(event).await {
                        return;
                    }
                }
                // Wake when new reset requests arrive.
                () = self.participant_registry.reset_notify().notified() => {}
                // Fire when the no-viewer grace period expires.
                () = async {
                    match idle_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending().await,
                    }
                } => {
                    info!(
                        remote_access_session_id,
                        "no active viewers within device_wait_for_viewer window; returning to dormant"
                    );
                    return;
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
                let sid = participant.sid();
                let joined_at = participant.joined_at();
                info!(
                    remote_access_session_id,
                    participant_identity = %participant_identity,
                    sid = %sid,
                    joined_at,
                    version = %version,
                    "participant active in room"
                );
                if let Err(e) = self
                    .add_participant(participant_identity, sid, joined_at)
                    .await
                {
                    error!(remote_access_session_id, error = %e, "failed to add participant: {e}");
                }
            }
            RoomEvent::ParticipantDisconnected(participant) => {
                let participant_identity = participant.identity();
                let sid = participant.sid();
                info!(
                    remote_access_session_id,
                    participant_identity = %participant_identity,
                    sid = %sid,
                    "participant disconnected from room"
                );
                // Match the disconnect against the specific LiveKit connection
                // instance we registered. If the stored `Participant` was added
                // for a *later* instance (same identity, different SID — a
                // reconnect we already reset to), the SID-keyed remove misses
                // and returns `None`: this event is stale and its target is
                // already gone.
                self.remove_participant(&sid);
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
    /// We guard against the common case by checking `remote_participants()` after
    /// removing: if LiveKit has already removed the participant, we skip the re-add
    /// and let the normal `ParticipantConnected` flow handle any future reconnection.
    /// Without this guard, re-adding would open a fresh stream whose first write would
    /// also fail, re-triggering the reset in an infinite loop.
    ///
    /// This is a best-effort check (TOCTOU): the participant could disconnect between
    /// the check and the `stream_bytes` call inside `add_participant`. In that narrow
    /// window, `add_participant` may open a dead stream, but the subsequent
    /// `ParticipantDisconnected` event will clean it up. This is harmless — just a
    /// wasted `stream_bytes` call and a log line.
    async fn reset_participant(self: &Arc<Self>, target_sid: ParticipantSid) {
        let remote_access_session_id = self.remote_access_session_id();

        // Remove by SID and capture identity from the returned participant.
        // The SID identifies the exact instance that requested the reset, so
        // a same-identity reconnect (which has a different SID) won't match
        // — `None` is the staleness filter.
        let Some(participant) = self.remove_participant(&target_sid) else {
            info!(
                remote_access_session_id,
                participant_sid = %target_sid,
                "reset requested for already-removed participant; skipping",
            );
            return;
        };
        let participant_id = participant.participant_id().clone();
        drop(participant);

        // Best-effort guard: skip re-add if LiveKit has already removed the participant
        // (e.g., because the underlying WebRTC connection dropped). In that case, the
        // `ParticipantDisconnected` event is already queued and a future reconnect will
        // go through the normal `ParticipantConnected` → `add_participant` path.
        //
        // If a new instance has already reconnected under the same identity,
        // its SID, attributes, and `joined_at` are what we re-register with
        // — so that (a) a later stale `ParticipantDisconnected` for the
        // *old* instance's SID won't match and tear down this
        // re-registration, (b) a protocol-version change between instances
        // is honoured rather than assumed-unchanged, and (c) the registry's
        // `joined_at` monotonicity check sees a value tied to this specific
        // instance.
        let Some((sid, attributes, joined_at)) = self
            .room
            .remote_participants()
            .get(&participant_id)
            .map(|p| (p.sid(), p.attributes(), p.joined_at()))
        else {
            info!(
                remote_access_session_id,
                participant_identity = %participant_id,
                "participant already left room, skipping re-add after control-plane failure",
            );
            return;
        };

        // Re-validate the protocol version against the freshly-queried
        // attributes. A same-identity reconnect could in principle bring a
        // different protocol version; trust the fresh value, just like we
        // trust the fresh SID.
        let Some(version) = protocol_version::check_participant_protocol_version(
            &participant_id,
            &attributes,
            remote_access_session_id,
        ) else {
            self.send_incompatible_version_error(&participant_id, &attributes)
                .await;
            return;
        };

        warn!(
            remote_access_session_id,
            participant_identity = %participant_id,
            sid = %sid,
            joined_at,
            version = %version,
            "resetting participant after control-plane failure",
        );
        if let Err(e) = self.add_participant(participant_id, sid, joined_at).await {
            error!(
                remote_access_session_id,
                error = %e,
                "failed to re-add participant after reset: {e}",
            );
        }
    }

    /// Periodically logs session statistics for monitoring and debugging.
    pub(super) async fn log_periodic_stats(&self) {
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

    /// Returns the currently-cached channel advertisements encoded as a single
    /// framed control-plane message, or `None` if no channels are advertised.
    fn encode_channel_advertisements(&self) -> Option<Bytes> {
        let state = self.state.read();
        let msg = state.with_channels(|channels| {
            let msg = advertise::advertise_channels(channels.values());
            if msg.channels.is_empty() {
                return None;
            }
            let mut msg = msg.into_owned();
            state.add_metadata_to_advertisement(&mut msg);
            Some(msg)
        })??;
        Some(encode_json_message(&msg))
    }

    /// Returns the currently-cached service advertisements encoded as a single
    /// framed control-plane message, or `None` if no services are registered.
    fn encode_service_advertisements(&self) -> Option<Bytes> {
        let services: Vec<_> = self.services.read().values().cloned().collect();
        build_advertise_services_msg(&services).map(|msg| encode_json_message(&msg))
    }

    /// Broadcasts service advertisements for the given service IDs to all connected participants.
    pub(super) fn advertise_new_services(&self, service_ids: &[ServiceId]) {
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
    pub(super) fn unadvertise_services(&self, service_ids: &[ServiceId]) {
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
        participant.send_control(encode_json_message(&failure));
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
        participant.send_control(encode_json_message(&msg));
    }

    /// Publish parameter values to all participants subscribed to those parameters.
    pub(super) fn publish_parameter_values(&self, parameters: Vec<Parameter>) {
        if !self.has_capability(Capability::Parameters) {
            error!("Server does not support parameters capability");
            return;
        }

        // Collect the per-participant messages, then send them after the locks
        // are released to minimize lock scope.
        let participants = self.participant_registry.collect_participants();
        let to_send: Vec<(Arc<Participant>, Bytes)> = {
            let state = self.state.read();
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
            participant.send_control(data);
        }
    }

    /// Publish a status message to all connected participants.
    pub(super) fn publish_status(&self, status: Status) {
        self.broadcast_control(encode_json_message(&status));
    }

    /// Remove status messages by ID from all connected participants.
    pub(super) fn remove_status(&self, status_ids: Vec<String>) {
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

        participant.send_control(encoded);
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
    pub(super) fn replace_connection_graph(&self, replacement_graph: ConnectionGraph) {
        let mut graph = self.connection_graph.lock();
        let update = graph.update(replacement_graph);
        let encoded = encode_json_message(&update);
        for participant in self.participant_registry.collect_participants() {
            if graph.is_subscriber(participant.client_id()) {
                participant.send_control(encoded.clone());
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
    /// Each track is named video-ch-{channel_id}.
    ///
    /// Caller must hold `subscription_lock`.
    fn start_video_tracks(self: &Arc<Self>, first_subscribed: &[ChannelId]) {
        let to_start: SmallVec<[(ChannelId, VideoInputSchema); 4]> = {
            let state = self.state.read();
            first_subscribed
                .iter()
                .filter_map(|&channel_id| {
                    let input_schema = state.get_video_schema(&channel_id)?;
                    Some((channel_id, input_schema))
                })
                .collect()
        };

        for (channel_id, input_schema) in to_start {
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

            let track_name = format!("video-ch-{}", u64::from(channel_id));
            let track = LocalVideoTrack::create_video_track(
                &track_name,
                RtcVideoSource::Native(video_source),
            );

            let local_participant = self.room.local_participant().clone();
            let session = self.clone();
            tokio::spawn(async move {
                let local_track = LocalTrack::Video(track);
                // Prefer H.264 so that the libwebrtc VAAPI encoder (H.264-only) can be used
                // on Linux hosts that have libva + a VA driver available. VP8/VP9/AV1 paths
                // are software-only in our builds, so H.264 is at worst parity elsewhere.
                let publish_options = TrackPublishOptions {
                    video_codec: VideoCodec::H264,
                    ..Default::default()
                };
                match local_participant
                    .publish_track(local_track, publish_options)
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
    ///
    /// Reliable channels should be excluded by the caller; their data goes via
    /// the control plane instead of data tracks.
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
    use std::collections::HashSet;

    use super::*;
    use crate::protocol::v2::server::FetchAssetResponse;
    use crate::remote_common::fetch_asset::{
        AssetHandler, AsyncAssetHandlerFn, BlockingAssetHandlerFn,
    };

    fn make_participant_with_rx(name: &str) -> (Arc<Participant>, flume::Receiver<Bytes>) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let identity = ParticipantIdentity(name.to_string());
        let sid = crate::remote_access::participant::test_sid(&format!("{name}-{n}"));
        let (tx, rx) = flume::bounded(16);
        let pending_resets = Arc::new(parking_lot::Mutex::new(HashSet::new()));
        let reset_notify = Arc::new(tokio::sync::Notify::new());
        let cancel = CancellationToken::new();
        let participant = Arc::new(Participant::new(
            identity,
            sid,
            tx,
            pending_resets,
            reset_notify,
            cancel,
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

    // ---- flush-task tests ----

    /// Spawns a participant with a test writer via `Participant::spawn`.
    /// Returns the participant (for sending), the test writer (for inspecting
    /// writes), and the flush-task's `JoinHandle`.
    fn spawn_test_participant(
        session_cancel: &CancellationToken,
    ) -> (
        Arc<Participant>,
        Arc<crate::remote_access::participant::TestByteStreamWriter>,
        tokio::task::JoinHandle<()>,
    ) {
        use crate::remote_access::participant::{
            ParticipantWriter, TestByteStreamWriter, test_sid,
        };

        let writer = Arc::new(TestByteStreamWriter::default());
        let pending_resets = Arc::new(parking_lot::Mutex::new(HashSet::new()));
        let reset_notify = Arc::new(tokio::sync::Notify::new());
        let (participant, handle) = Participant::spawn(
            ParticipantIdentity("test".to_string()),
            test_sid("flush-test"),
            0,
            ParticipantWriter::Test(writer.clone()),
            DEFAULT_MESSAGE_BACKLOG_SIZE,
            pending_resets,
            reset_notify,
            session_cancel,
        );
        (participant, writer, handle)
    }

    #[tokio::test]
    async fn flush_task_delivers_messages() {
        let cancel = CancellationToken::new();
        let (participant, writer, handle) = spawn_test_participant(&cancel);

        participant.send_control(Bytes::from_static(b"hello"));
        participant.send_control(Bytes::from_static(b"world"));

        // Drop the participant to signal the flush-task to exit.
        drop(participant);
        handle.await.unwrap();

        let writes = writer.writes();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0], Bytes::from_static(b"hello"));
        assert_eq!(writes[1], Bytes::from_static(b"world"));
    }

    #[tokio::test]
    async fn flush_task_stops_on_sender_drop() {
        let cancel = CancellationToken::new();
        let (participant, _writer, handle) = spawn_test_participant(&cancel);

        // Drop the participant without cancelling — task should exit because recv returns Err.
        drop(participant);

        let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(result.is_ok(), "flush-task did not exit after sender drop");
    }

    #[tokio::test]
    async fn flush_task_stops_on_cancellation() {
        let cancel = CancellationToken::new();
        let (_participant, _writer, handle) = spawn_test_participant(&cancel);

        // Cancel without dropping the participant — task should exit via the select! arm.
        cancel.cancel();

        let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(result.is_ok(), "flush-task did not exit after cancellation");
    }

    #[tokio::test]
    async fn flush_tasks_are_independent() {
        // Two participants spawned independently. Dropping one and awaiting its
        // flush-task should not affect the other.
        let cancel = CancellationToken::new();
        let (participant_a, writer_a, handle_a) = spawn_test_participant(&cancel);
        let (participant_b, writer_b, handle_b) = spawn_test_participant(&cancel);

        // Send a message to both.
        participant_a.send_control(Bytes::from_static(b"msg_a"));
        participant_b.send_control(Bytes::from_static(b"msg_b"));

        // Drop B's participant so it flushes and exits.
        drop(participant_b);
        let result = tokio::time::timeout(Duration::from_secs(1), handle_b).await;
        assert!(
            result.is_ok(),
            "task B should complete independently of task A"
        );
        assert_eq!(writer_b.writes(), vec![Bytes::from_static(b"msg_b")]);

        // A should also have written (TestByteStreamWriter is instant).
        drop(participant_a);
        let result = tokio::time::timeout(Duration::from_secs(1), handle_a).await;
        assert!(result.is_ok(), "task A should complete after drop");
        assert_eq!(writer_a.writes(), vec![Bytes::from_static(b"msg_a")]);
    }

    fn make_test_participant(queue_size: usize) -> (Participant, flume::Receiver<Bytes>) {
        let (tx, rx) = flume::bounded::<Bytes>(queue_size);
        let pending_resets = Arc::new(parking_lot::Mutex::new(HashSet::new()));
        let reset_notify = Arc::new(tokio::sync::Notify::new());
        let cancel = CancellationToken::new();
        let participant = Participant::new(
            ParticipantIdentity("alice".to_string()),
            crate::remote_access::participant::test_sid("alice"),
            tx,
            pending_resets,
            reset_notify,
            cancel,
        );
        (participant, rx)
    }

    #[test]
    fn try_queue_control_returns_false_when_full() {
        let (participant, _rx) = make_test_participant(1);

        // First message fits.
        assert!(participant.try_queue_control(Bytes::from_static(b"first")));
        // Second message overflows the 1-slot queue.
        assert!(!participant.try_queue_control(Bytes::from_static(b"second")));
    }

    #[test]
    fn try_queue_control_returns_true_when_disconnected() {
        let (participant, rx) = make_test_participant(1);

        // Drop the receiver — channel disconnected.
        drop(rx);
        // Disconnected returns true (no reset needed).
        assert!(participant.try_queue_control(Bytes::from_static(b"msg")));
    }
}

//! Shared test infrastructure for remote access integration tests.
//!
//! Provides helpers for connecting to LiveKit rooms, reading control channel frames,
//! managing test gateway instances, and common utilities. Used across test suites
//! such as `livekit_test` and `netem_test`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use futures_util::StreamExt;
use livekit::id::ParticipantIdentity;
use livekit::prelude::{DataTrackStream, RemoteDataTrack};
use livekit::{
    ByteStreamWriter, Room, RoomEvent, RoomOptions, StreamByteOptions, StreamWriter as _,
};
use tokio::sync::Mutex;
use tracing::info;

use foxglove::protocol::v2::BinaryMessage;
use foxglove::protocol::v2::client::{
    Advertise, AdvertiseChannel, GetParameters, MessageData as ClientMessageData,
    ServiceCallRequest, SetParameters, Subscribe, SubscribeChannel, SubscribeParameterUpdates,
    Unadvertise, Unsubscribe, UnsubscribeParameterUpdates,
};
use foxglove::protocol::v2::server::MessageData as ServerMessageData;

/// Describes a client-advertised channel for use in test helpers.
pub struct ClientChannelDesc {
    pub id: u32,
    pub topic: String,
    pub encoding: String,
    pub schema_name: String,
}
use foxglove::protocol::v2::server::ServerMessage;
use foxglove::remote_access::service::Service;

use crate::frame::{self, Frame, OpCode};
use crate::{livekit_token, mock_server};

/// Default timeout for waiting for events or stream data.
pub const EVENT_TIMEOUT: Duration = Duration::from_secs(15);
/// Longer timeout for netem-impaired connections where gateway startup and
/// WebRTC negotiation are significantly slower.
pub const NETEM_EVENT_TIMEOUT: Duration = Duration::from_secs(30);
/// Default timeout for reading frames from the byte stream.
pub const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Default timeout for gateway shutdown.
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
/// Polling interval for condition checks.
pub const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Per-attempt timeout when waiting for a byte stream during connection retries.
pub const CONNECT_RETRY_TIMEOUT: Duration = Duration::from_secs(5);

/// Type alias for a channel filter function passed to [`TestGateway::start_with_filter`].
pub type ChannelFilterFn =
    Box<dyn Fn(&foxglove::ChannelDescriptor) -> bool + Send + Sync + 'static>;

// ---------------------------------------------------------------------------
// FrameReader: accumulates bytes from a LiveKit byte stream reader and
// parses successive byte stream frames.
// ---------------------------------------------------------------------------

/// Reads chunks from a LiveKit byte stream and parses byte stream frames.
pub struct FrameReader {
    reader: livekit::ByteStreamReader,
    buf: Vec<u8>,
}

impl FrameReader {
    pub fn new(reader: livekit::ByteStreamReader) -> Self {
        Self {
            reader,
            buf: Vec::new(),
        }
    }

    /// Reads chunks until a complete frame is available and returns it.
    pub async fn next_frame(&mut self) -> Result<Frame> {
        let deadline = tokio::time::Instant::now() + READ_TIMEOUT;
        loop {
            // Check if we already have a complete frame buffered.
            if let Some((frame, consumed)) = frame::try_parse_frame(&self.buf)? {
                self.buf.drain(..consumed);
                return Ok(frame);
            }
            let chunk = tokio::time::timeout_at(deadline, self.reader.next())
                .await
                .context("timeout reading byte stream chunks")?
                .context("byte stream ended unexpectedly")?
                .map_err(|e| anyhow::anyhow!("stream read error: {e}"))?;

            self.buf.extend_from_slice(&chunk);
        }
    }

    /// Reads the next frame and parses it as a [`ServerMessage`].
    pub async fn next_server_message(&mut self) -> Result<ServerMessage<'static>> {
        let frame = self.next_frame().await?;
        match frame.op_code {
            OpCode::Text => {
                let json =
                    std::str::from_utf8(&frame.payload).context("invalid UTF-8 in text frame")?;
                Ok(ServerMessage::parse_json(json)
                    .context("failed to parse server JSON message")?
                    .into_owned())
            }
            OpCode::Binary => Ok(ServerMessage::parse_binary(&frame.payload)
                .context("failed to parse server binary message")?
                .into_owned()),
        }
    }
}

// ---------------------------------------------------------------------------
// DeviceChannelReader: reads data track frames for a device channel and
// converts them to MessageData.
// ---------------------------------------------------------------------------

const DATA_TRACK_FRAME_HEADER_SIZE: usize = 8; // u16 LE flags + u16 LE data_offset + u32 LE sequence

/// Reads framed data track frames and converts them to [`ServerMessageData`].
///
/// Each frame payload contains an 8-byte header (u16 LE flags, u16 LE data_offset,
/// u32 LE sequence) followed by the raw message data. The channel identity is
/// determined by the data track name (topic), not by the frame content.
pub struct DeviceChannelReader {
    stream: DataTrackStream,
}

impl DeviceChannelReader {
    /// Reads the next frame from the data track and constructs a [`ServerMessageData`].
    ///
    /// The frame header contains a sequence number (not a channel_id); channel identity
    /// is determined by which data track this reader is attached to.
    pub async fn next_message_data(&mut self) -> Result<ServerMessageData<'static>> {
        let deadline = tokio::time::Instant::now() + READ_TIMEOUT;
        let frame = tokio::time::timeout_at(deadline, self.stream.next())
            .await
            .context("timeout reading data track frame")?
            .context("data track stream ended")?;
        let payload = frame.payload();
        anyhow::ensure!(
            payload.len() >= DATA_TRACK_FRAME_HEADER_SIZE,
            "data track frame too small ({} bytes)",
            payload.len()
        );
        let data = payload[DATA_TRACK_FRAME_HEADER_SIZE..].to_vec();
        let log_time = frame.user_timestamp().unwrap_or(0);
        Ok(ServerMessageData::new(0, log_time, data))
    }
}

// ---------------------------------------------------------------------------
// ViewerConnection: connects to a LiveKit room and provides helpers for
// reading control channel messages.
// ---------------------------------------------------------------------------

/// A viewer connected to a LiveKit room with an open control channel byte stream.
///
/// Maintains a persistent `ByteStreamWriter` for sending control messages to the gateway,
/// reusing a single stream rather than opening a new one per message.
pub struct ViewerConnection {
    pub room: Room,
    pub events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    pub frame_reader: FrameReader,
    /// Cached writer for the control channel byte stream to the gateway.
    /// Lazily opened on first send, reused for all subsequent messages.
    control_writer: Mutex<Option<ByteStreamWriter>>,
    /// Buffered `DataTrackPublished` events that were received while processing
    /// other room events. Checked before waiting on the events channel.
    pending_data_tracks: Vec<RemoteDataTrack>,
    /// Per-channel device data track readers, keyed by channel ID.
    device_channel_readers: HashMap<u64, DeviceChannelReader>,
}

impl ViewerConnection {
    /// Constructs a `ViewerConnection` from a pre-connected room by waiting for the
    /// gateway to open a control channel byte stream. Unlike [`connect`], this does not
    /// retry the room connection — useful when the viewer must remain in the room while
    /// the gateway joins (e.g., testing advertisement to existing participants).
    pub async fn from_room(
        room: Room,
        mut events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    ) -> Result<Self> {
        let deadline = tokio::time::Instant::now() + EVENT_TIMEOUT;
        let reader = loop {
            let event = tokio::time::timeout_at(deadline, events.recv())
                .await
                .context("timeout waiting for gateway to open byte stream")?
                .context("room events channel closed")?;
            if let RoomEvent::ByteStreamOpened {
                reader: stream_reader,
                topic,
                ..
            } = event
            {
                if topic == "control" {
                    break stream_reader.take().context("reader already taken")?;
                }
            }
        };
        Ok(Self {
            room,
            events,
            frame_reader: FrameReader::new(reader),
            control_writer: Mutex::new(None),
            pending_data_tracks: Vec::new(),
            device_channel_readers: HashMap::new(),
        })
    }

    /// Connects a viewer to the LiveKit room and waits for the control channel
    /// byte stream to open. Retries the connection if the gateway hasn't
    /// joined the room yet (no ByteStreamOpened within a short window).
    pub async fn connect(room_name: &str, viewer_identity: &str) -> Result<Self> {
        Self::connect_with_timeout(room_name, viewer_identity, EVENT_TIMEOUT).await
    }

    /// Like [`connect`](Self::connect), but with an explicit overall timeout.
    /// Use [`NETEM_EVENT_TIMEOUT`] for tests running under network impairment.
    pub async fn connect_with_timeout(
        room_name: &str,
        viewer_identity: &str,
        timeout: Duration,
    ) -> Result<Self> {
        let outer_deadline = tokio::time::Instant::now() + timeout;
        loop {
            let token = livekit_token::generate_token(room_name, viewer_identity)?;
            let (room, mut events) = Room::connect(
                &livekit_token::livekit_url(),
                &token,
                RoomOptions::default(),
            )
            .await
            .context("viewer failed to connect to LiveKit")?;
            info!("{viewer_identity} connected to room, waiting for byte stream");

            // Wait for a ByteStreamOpened event. Use a shorter inner timeout so
            // we can retry the connection if the gateway hasn't joined yet.
            // Cap to the outer deadline so the final attempt uses all remaining
            // time.
            let inner_deadline = std::cmp::min(
                tokio::time::Instant::now() + CONNECT_RETRY_TIMEOUT,
                outer_deadline,
            );
            let mut buffered_data_tracks = Vec::new();
            let reader = loop {
                let event = tokio::time::timeout_at(inner_deadline, events.recv()).await;
                match event {
                    Err(_) => break None, // inner timeout — retry
                    Ok(None) => anyhow::bail!("room events channel closed"),
                    Ok(Some(RoomEvent::ByteStreamOpened {
                        reader: stream_reader,
                        topic,
                        ..
                    })) if topic == "control" => {
                        break Some(stream_reader.take().context("reader already taken")?);
                    }
                    Ok(Some(RoomEvent::DataTrackPublished(track))) => {
                        buffered_data_tracks.push(track);
                    }
                    Ok(Some(_)) => continue,
                }
            };

            if let Some(reader) = reader {
                return Ok(Self {
                    room,
                    events,
                    frame_reader: FrameReader::new(reader),
                    control_writer: Mutex::new(None),
                    pending_data_tracks: buffered_data_tracks,
                    device_channel_readers: HashMap::new(),
                });
            }

            // Gateway not ready yet — close and retry.
            let _ = room.close().await;
            if tokio::time::Instant::now() >= outer_deadline {
                anyhow::bail!("timeout waiting for gateway to open byte stream");
            }
            info!("{viewer_identity} retrying connection (gateway not ready)");
        }
    }

    /// Reads and validates the initial ServerInfo message.
    pub async fn expect_server_info(
        &mut self,
    ) -> Result<foxglove::protocol::v2::server::ServerInfo> {
        let msg = self.frame_reader.next_server_message().await?;
        match msg {
            ServerMessage::ServerInfo(info) => Ok(info),
            other => anyhow::bail!("expected ServerInfo, got: {other:?}"),
        }
    }

    /// Reads and returns the next Advertise message.
    pub async fn expect_advertise(
        &mut self,
    ) -> Result<foxglove::protocol::v2::server::Advertise<'static>> {
        let msg = self.frame_reader.next_server_message().await?;
        match msg {
            ServerMessage::Advertise(adv) => Ok(adv),
            other => anyhow::bail!("expected Advertise, got: {other:?}"),
        }
    }

    /// Reads and returns the next Status message.
    pub async fn expect_status(&mut self) -> Result<foxglove::protocol::v2::server::Status> {
        let msg = self.frame_reader.next_server_message().await?;
        match msg {
            ServerMessage::Status(status) => Ok(status),
            other => anyhow::bail!("expected Status, got: {other:?}"),
        }
    }

    /// Reads and returns the next RemoveStatus message.
    pub async fn expect_remove_status(
        &mut self,
    ) -> Result<foxglove::protocol::v2::server::RemoveStatus> {
        let msg = self.frame_reader.next_server_message().await?;
        match msg {
            ServerMessage::RemoveStatus(remove) => Ok(remove),
            other => anyhow::bail!("expected RemoveStatus, got: {other:?}"),
        }
    }

    /// Reads and returns the next Unadvertise message.
    pub async fn expect_unadvertise(
        &mut self,
    ) -> Result<foxglove::protocol::v2::server::Unadvertise> {
        let msg = self.frame_reader.next_server_message().await?;
        match msg {
            ServerMessage::Unadvertise(unadv) => Ok(unadv),
            other => anyhow::bail!("expected Unadvertise, got: {other:?}"),
        }
    }

    /// Reads and returns the next MessageData message.
    pub async fn expect_message_data(
        &mut self,
    ) -> Result<foxglove::protocol::v2::server::MessageData<'static>> {
        let msg = self.frame_reader.next_server_message().await?;
        match msg {
            ServerMessage::MessageData(data) => Ok(data),
            other => anyhow::bail!("expected MessageData, got: {other:?}"),
        }
    }

    /// Reads and returns the next AdvertiseServices message.
    pub async fn expect_advertise_services(
        &mut self,
    ) -> Result<foxglove::protocol::v2::server::AdvertiseServices<'static>> {
        let msg = self.frame_reader.next_server_message().await?;
        match msg {
            ServerMessage::AdvertiseServices(adv) => Ok(adv),
            other => anyhow::bail!("expected AdvertiseServices, got: {other:?}"),
        }
    }

    /// Reads and returns the next UnadvertiseServices message.
    pub async fn expect_unadvertise_services(
        &mut self,
    ) -> Result<foxglove::protocol::v2::server::UnadvertiseServices> {
        let msg = self.frame_reader.next_server_message().await?;
        match msg {
            ServerMessage::UnadvertiseServices(unadv) => Ok(unadv),
            other => anyhow::bail!("expected UnadvertiseServices, got: {other:?}"),
        }
    }

    /// Reads and returns the next ServiceCallResponse message.
    pub async fn expect_service_call_response(
        &mut self,
    ) -> Result<foxglove::protocol::v2::server::ServiceCallResponse<'static>> {
        let msg = self.frame_reader.next_server_message().await?;
        match msg {
            ServerMessage::ServiceCallResponse(resp) => Ok(resp),
            other => anyhow::bail!("expected ServiceCallResponse, got: {other:?}"),
        }
    }

    /// Reads and returns the next ServiceCallFailure message.
    pub async fn expect_service_call_failure(
        &mut self,
    ) -> Result<foxglove::protocol::v2::server::ServiceCallFailure> {
        let msg = self.frame_reader.next_server_message().await?;
        match msg {
            ServerMessage::ServiceCallFailure(fail) => Ok(fail),
            other => anyhow::bail!("expected ServiceCallFailure, got: {other:?}"),
        }
    }

    /// Sends a pre-framed message to the gateway over the persistent control channel byte stream.
    /// Opens the stream lazily on first call and reuses it for all subsequent messages.
    async fn send_framed_message(&self, framed: &[u8]) -> Result<()> {
        let mut guard = self.control_writer.lock().await;
        if guard.is_none() {
            let gateway_identity = ParticipantIdentity(mock_server::TEST_DEVICE_ID.to_string());
            let writer = self
                .room
                .local_participant()
                .stream_bytes(StreamByteOptions {
                    topic: "control".to_string(),
                    destination_identities: vec![gateway_identity],
                    ..StreamByteOptions::default()
                })
                .await
                .map_err(|e| anyhow::anyhow!("failed to open byte stream to gateway: {e}"))?;
            *guard = Some(writer);
        }
        guard
            .as_ref()
            .unwrap()
            .write(framed)
            .await
            .map_err(|e| anyhow::anyhow!("failed to write message to gateway: {e}"))?;
        Ok(())
    }

    /// Sends a binary-framed ServiceCallRequest to the gateway.
    pub async fn send_service_call_request(&self, req: &ServiceCallRequest<'_>) -> Result<()> {
        let binary = req.to_bytes();
        let framed = frame::frame_binary_message(&binary);
        self.send_framed_message(&framed).await
    }

    /// Sends a JSON-framed Subscribe message to the gateway.
    pub async fn send_subscribe(&self, channel_ids: &[u64]) -> Result<()> {
        self.send_subscribe_channels(
            channel_ids
                .iter()
                .map(|&id| SubscribeChannel {
                    id,
                    request_video_track: false,
                })
                .collect(),
        )
        .await
    }

    /// Sends a JSON-framed Subscribe with explicit channel options.
    pub async fn send_subscribe_channels(&self, channels: Vec<SubscribeChannel>) -> Result<()> {
        let subscribe = Subscribe { channels };
        let json = serde_json::to_string(&subscribe)?;
        let framed = frame::frame_text_message(json.as_bytes());
        self.send_framed_message(&framed).await
    }

    /// Sends a Subscribe and waits for the channel to have at least one sink.
    pub async fn subscribe_and_wait(
        &self,
        channel_ids: &[u64],
        channel: &foxglove::RawChannel,
    ) -> Result<()> {
        self.send_subscribe(channel_ids).await?;
        poll_until(|| channel.has_sinks()).await;
        Ok(())
    }

    /// Sends a Subscribe with video requested and waits for the channel to have at least one sink.
    pub async fn subscribe_video_and_wait(
        &self,
        channel_ids: &[u64],
        channel: &foxglove::RawChannel,
    ) -> Result<()> {
        let channels = channel_ids
            .iter()
            .map(|&id| SubscribeChannel {
                id,
                request_video_track: true,
            })
            .collect();
        self.send_subscribe_channels(channels).await?;
        poll_until(|| channel.has_sinks()).await;
        Ok(())
    }

    /// Sends a JSON-framed Unsubscribe message to the gateway.
    pub async fn send_unsubscribe(&self, channel_ids: &[u64]) -> Result<()> {
        let unsubscribe = Unsubscribe::new(channel_ids.iter().copied());
        let json = serde_json::to_string(&unsubscribe)?;
        let framed = frame::frame_text_message(json.as_bytes());
        self.send_framed_message(&framed).await
    }

    /// Sends a JSON-framed client Advertise message to the gateway.
    pub async fn send_client_advertise(&self, channels: &[ClientChannelDesc]) -> Result<()> {
        let advertise = Advertise::new(channels.iter().map(|c| AdvertiseChannel {
            id: c.id,
            topic: c.topic.as_str().into(),
            encoding: c.encoding.as_str().into(),
            schema_name: c.schema_name.as_str().into(),
            schema_encoding: None,
            schema: None,
        }));
        let json_str = serde_json::to_string(&advertise)?;
        let framed = frame::frame_text_message(json_str.as_bytes());
        self.send_framed_message(&framed).await
    }

    /// Sends a JSON-framed client Unadvertise message to the gateway.
    pub async fn send_client_unadvertise(&self, channel_ids: &[u32]) -> Result<()> {
        let unadvertise = Unadvertise::new(channel_ids.iter().copied());
        let json = serde_json::to_string(&unadvertise)?;
        let framed = frame::frame_text_message(json.as_bytes());
        self.send_framed_message(&framed).await
    }

    /// Sends a binary-framed `ClientMessageData` on the control channel.
    pub async fn send_client_message_data(&self, channel_id: u32, data: &[u8]) -> Result<()> {
        let msg = ClientMessageData::new(channel_id, data);
        let inner = msg.to_bytes();
        let framed = frame::frame_binary_message(&inner);
        self.send_framed_message(&framed).await
    }

    /// Waits for a `TrackSubscribed` room event and returns the track name.
    pub async fn expect_track_subscribed(&mut self) -> Result<String> {
        let deadline = tokio::time::Instant::now() + EVENT_TIMEOUT;
        loop {
            let event = tokio::time::timeout_at(deadline, self.events.recv())
                .await
                .context("timeout waiting for TrackSubscribed event")?
                .context("room events channel closed")?;
            match event {
                RoomEvent::TrackSubscribed { publication, .. } => {
                    return Ok(publication.name());
                }
                RoomEvent::DataTrackPublished(track) => {
                    self.pending_data_tracks.push(track);
                }
                _ => continue,
            }
        }
    }

    /// Waits for a `TrackUnsubscribed` room event and returns the track name.
    pub async fn expect_track_unsubscribed(&mut self) -> Result<String> {
        let deadline = tokio::time::Instant::now() + EVENT_TIMEOUT;
        loop {
            let event = tokio::time::timeout_at(deadline, self.events.recv())
                .await
                .context("timeout waiting for TrackUnsubscribed event")?
                .context("room events channel closed")?;
            match event {
                RoomEvent::TrackUnsubscribed { publication, .. } => {
                    return Ok(publication.name());
                }
                RoomEvent::DataTrackPublished(track) => {
                    self.pending_data_tracks.push(track);
                }
                _ => continue,
            }
        }
    }

    /// Waits for a `DataTrackPublished` event whose track name matches
    /// `data-ch-{channel_id}`, subscribes to it, and returns a [`DeviceChannelReader`].
    ///
    /// Non-matching tracks are buffered for later consumption.
    pub async fn expect_device_channel_data_track(
        &mut self,
        channel_id: u64,
    ) -> Result<DeviceChannelReader> {
        let expected_name = format!("data-ch-{channel_id}");
        if let Some(idx) = self
            .pending_data_tracks
            .iter()
            .position(|t| t.info().name() == expected_name)
        {
            let track = self.pending_data_tracks.remove(idx);
            return subscribe_to_device_data_track(track).await;
        }

        let deadline = tokio::time::Instant::now() + READ_TIMEOUT;
        loop {
            let event = tokio::time::timeout_at(deadline, self.events.recv())
                .await
                .context("timeout waiting for device channel data track")?
                .context("room events channel closed")?;
            match event {
                RoomEvent::DataTrackPublished(track) if track.info().name() == expected_name => {
                    return subscribe_to_device_data_track(track).await;
                }
                RoomEvent::DataTrackPublished(track) => {
                    self.pending_data_tracks.push(track);
                }
                _ => continue,
            }
        }
    }

    /// Ensures a device channel data track reader exists for `channel_id`.
    ///
    /// If one already exists for this channel, this is a no-op. Otherwise,
    /// waits for a `DataTrackPublished` event whose track name matches
    /// `data-ch-{channel_id}` and subscribes.
    pub async fn ensure_device_data_track(&mut self, channel_id: u64) -> Result<()> {
        if self.device_channel_readers.contains_key(&channel_id) {
            return Ok(());
        }
        let reader = self.expect_device_channel_data_track(channel_id).await?;
        self.device_channel_readers.insert(channel_id, reader);
        Ok(())
    }

    /// Reads the next MessageData from the device channel data track for `channel_id`.
    ///
    /// If no data track reader is stored yet for this channel, waits for a
    /// `DataTrackPublished` event for `channel_id` and subscribes first.
    /// Subsequent calls reuse the same reader.
    pub async fn expect_new_data_track_and_message_data(
        &mut self,
        channel_id: u64,
    ) -> Result<ServerMessageData<'static>> {
        self.ensure_device_data_track(channel_id).await?;
        self.device_channel_readers
            .get_mut(&channel_id)
            .unwrap()
            .next_message_data()
            .await
    }

    /// Waits for a `ParticipantDisconnected` room event for the given identity.
    ///
    /// Used to synchronize on a participant's departure before sending further messages,
    /// ensuring the gateway has had a chance to update its subscription state.
    pub async fn wait_for_participant_disconnected(&mut self, identity: &str) -> Result<()> {
        let deadline = tokio::time::Instant::now() + EVENT_TIMEOUT;
        loop {
            let event = tokio::time::timeout_at(deadline, self.events.recv())
                .await
                .context("timeout waiting for ParticipantDisconnected event")?
                .context("room events channel closed")?;
            match event {
                RoomEvent::ParticipantDisconnected(participant)
                    if participant.identity().0 == identity =>
                {
                    return Ok(());
                }
                RoomEvent::DataTrackPublished(track) => {
                    self.pending_data_tracks.push(track);
                }
                _ => continue,
            }
        }
    }

    /// Reads and returns the next ConnectionGraphUpdate message.
    pub async fn expect_connection_graph_update(
        &mut self,
    ) -> Result<foxglove::protocol::v2::server::ConnectionGraphUpdate> {
        let msg = self.frame_reader.next_server_message().await?;
        match msg {
            ServerMessage::ConnectionGraphUpdate(update) => Ok(update),
            other => anyhow::bail!("expected ConnectionGraphUpdate, got: {other:?}"),
        }
    }

    /// Sends a JSON-framed SubscribeConnectionGraph message to the gateway.
    pub async fn send_subscribe_connection_graph(&self) -> Result<()> {
        let json = r#"{"op":"subscribeConnectionGraph"}"#;
        let framed = frame::frame_text_message(json.as_bytes());
        self.send_framed_message(&framed).await
    }

    /// Sends a JSON-framed UnsubscribeConnectionGraph message to the gateway.
    pub async fn send_unsubscribe_connection_graph(&self) -> Result<()> {
        let json = r#"{"op":"unsubscribeConnectionGraph"}"#;
        let framed = frame::frame_text_message(json.as_bytes());
        self.send_framed_message(&framed).await
    }

    /// Reads and returns the next ParameterValues message.
    pub async fn expect_parameter_values(
        &mut self,
    ) -> Result<foxglove::protocol::v2::server::ParameterValues> {
        let msg = self.frame_reader.next_server_message().await?;
        match msg {
            ServerMessage::ParameterValues(params) => Ok(params),
            other => anyhow::bail!("expected ParameterValues, got: {other:?}"),
        }
    }

    /// Sends a JSON-framed message to the gateway.
    async fn send_json_message(&self, json: &str) -> Result<()> {
        let framed = frame::frame_text_message(json.as_bytes());
        self.send_framed_message(&framed).await
    }

    /// Sends a GetParameters request to the gateway.
    pub async fn send_get_parameters(&self, names: &[&str]) -> Result<()> {
        let msg = GetParameters::new(names.iter().copied());
        self.send_json_message(&serde_json::to_string(&msg)?).await
    }

    /// Sends a GetParameters request with a request ID to the gateway.
    pub async fn send_get_parameters_with_id(&self, names: &[&str], id: &str) -> Result<()> {
        let msg = GetParameters::new(names.iter().copied()).with_id(id);
        self.send_json_message(&serde_json::to_string(&msg)?).await
    }

    /// Sends a SetParameters request to the gateway.
    pub async fn send_set_parameters(
        &self,
        parameters: Vec<foxglove::remote_access::Parameter>,
    ) -> Result<()> {
        let msg = SetParameters::new(parameters);
        self.send_json_message(&serde_json::to_string(&msg)?).await
    }

    /// Sends a SetParameters request with a request ID to the gateway.
    pub async fn send_set_parameters_with_id(
        &self,
        parameters: Vec<foxglove::remote_access::Parameter>,
        id: &str,
    ) -> Result<()> {
        let msg = SetParameters::new(parameters).with_id(id);
        self.send_json_message(&serde_json::to_string(&msg)?).await
    }

    /// Sends a SubscribeParameterUpdates request to the gateway.
    pub async fn send_subscribe_parameter_updates(&self, names: &[&str]) -> Result<()> {
        let msg = SubscribeParameterUpdates::new(names.iter().copied());
        self.send_json_message(&serde_json::to_string(&msg)?).await
    }

    /// Sends an UnsubscribeParameterUpdates request to the gateway.
    pub async fn send_unsubscribe_parameter_updates(&self, names: &[&str]) -> Result<()> {
        let msg = UnsubscribeParameterUpdates::new(names.iter().copied());
        self.send_json_message(&serde_json::to_string(&msg)?).await
    }

    pub async fn close(self) -> Result<()> {
        self.room
            .close()
            .await
            .context("failed to close viewer room")?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TestGateway: starts a mock server + Gateway for integration tests.
// ---------------------------------------------------------------------------

/// Options for starting a [`TestGateway`].
#[derive(Default)]
pub struct TestGatewayOptions {
    pub filter: Option<ChannelFilterFn>,
    pub listener: Option<Arc<dyn foxglove::remote_access::Listener>>,
    pub capabilities: Vec<foxglove::remote_access::Capability>,
    pub services: Vec<Service>,
}

/// A test gateway backed by a mock Foxglove API server and a LiveKit room.
pub struct TestGateway {
    pub room_name: String,
    pub _mock: mock_server::MockServerHandle,
    pub handle: foxglove::remote_access::GatewayHandle,
}

impl TestGateway {
    /// Starts a mock server + Gateway with the given context.
    pub async fn start(ctx: &Arc<foxglove::Context>) -> Result<Self> {
        Self::start_with_filter(ctx, None).await
    }

    /// Starts a mock server + Gateway with the given context and optional channel filter.
    pub async fn start_with_filter(
        ctx: &Arc<foxglove::Context>,
        filter: Option<ChannelFilterFn>,
    ) -> Result<Self> {
        let (room_name, mock) = Self::prepare().await;
        Self::start_with_mock(
            ctx,
            room_name,
            mock,
            TestGatewayOptions {
                filter,
                ..Default::default()
            },
        )
    }

    /// Starts a mock server + Gateway with the given context and options.
    pub async fn start_with_options(
        ctx: &Arc<foxglove::Context>,
        options: TestGatewayOptions,
    ) -> Result<Self> {
        let (room_name, mock) = Self::prepare().await;
        Self::start_with_mock(ctx, room_name, mock, options)
    }

    /// Creates a mock server and room name without starting the gateway.
    /// Use this when you need to perform setup (e.g., connecting a viewer)
    /// before the gateway joins the room.
    pub async fn prepare() -> (String, mock_server::MockServerHandle) {
        let room_name = format!("test-room-{}", unique_id());
        let mock = mock_server::start_mock_server(&room_name).await;
        info!("mock server started at {}", mock.url());
        (room_name, mock)
    }

    /// Starts the gateway using a pre-created mock server, room name, and options.
    pub fn start_with_mock(
        ctx: &Arc<foxglove::Context>,
        room_name: String,
        mock: mock_server::MockServerHandle,
        options: TestGatewayOptions,
    ) -> Result<Self> {
        let mut gateway = foxglove::remote_access::Gateway::new()
            .name(format!("test-device-{}", unique_id()))
            .device_token(mock_server::TEST_DEVICE_TOKEN)
            .foxglove_api_url(mock.url())
            .supported_encodings(["json"])
            .context(ctx);

        if let Some(f) = options.filter {
            gateway = gateway.channel_filter_fn(f);
        }
        if let Some(listener) = options.listener {
            gateway = gateway.listener(listener);
        }
        if !options.capabilities.is_empty() {
            gateway = gateway.capabilities(options.capabilities);
        }
        if !options.services.is_empty() {
            gateway = gateway.services(options.services);
        }

        let handle = gateway.start().context("start Gateway")?;

        Ok(Self {
            room_name,
            _mock: mock,
            handle,
        })
    }

    pub async fn stop(self) -> Result<()> {
        let runner = self.handle.stop();
        tokio::time::timeout(SHUTDOWN_TIMEOUT, runner)
            .await
            .context("timeout waiting for gateway to stop")?
            .context("gateway runner panicked")?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Data track helpers
// ---------------------------------------------------------------------------

/// Subscribes to a `RemoteDataTrack` and returns a [`DeviceChannelReader`].
async fn subscribe_to_device_data_track(track: RemoteDataTrack) -> Result<DeviceChannelReader> {
    let name = track.info().name().to_string();
    let stream = track
        .subscribe()
        .await
        .map_err(|e| anyhow::anyhow!("failed to subscribe to data track: {e}"))?;
    info!("subscribed to device data track: {name}");
    Ok(DeviceChannelReader { stream })
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Polls `cond` until it returns true, or panics after [`EVENT_TIMEOUT`].
pub async fn poll_until(cond: impl Fn() -> bool) {
    poll_until_timeout(cond, EVENT_TIMEOUT).await;
}

/// Polls `cond` until it returns true, or panics after `timeout`.
pub async fn poll_until_timeout(cond: impl Fn() -> bool, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    while !cond() {
        if tokio::time::Instant::now() >= deadline {
            panic!("poll_until condition not met within {timeout:?}");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Generates a unique identifier for use in room names.
pub fn unique_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    format!("{nanos:x}-{pid:x}")
}

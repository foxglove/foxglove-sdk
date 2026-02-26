//! Shared test infrastructure for remote access integration tests.
//!
//! Provides helpers for connecting to LiveKit rooms, reading ws-protocol frames,
//! managing test gateway instances, and common utilities. Used across test suites
//! such as `livekit_test` and `netem_test`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use futures_util::StreamExt;
use livekit::id::ParticipantIdentity;
use livekit::{Room, RoomEvent, RoomOptions, StreamByteOptions, StreamWriter as _};
use tracing::info;

use foxglove::protocol::v2::BinaryMessage;
use foxglove::protocol::v2::client::{Subscribe, Unsubscribe};
use foxglove::protocol::v2::server::ServerMessage;

use crate::frame::{self, Frame, OpCode};
use crate::{livekit_token, mock_server};

/// Default timeout for waiting for events or stream data.
pub const EVENT_TIMEOUT: Duration = Duration::from_secs(15);
/// Default timeout for reading frames from the byte stream.
pub const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Default timeout for gateway shutdown.
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
/// Polling interval for condition checks.
pub const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Type alias for a channel filter function passed to [`TestGateway::start_with_filter`].
pub type ChannelFilterFn =
    Box<dyn Fn(&foxglove::ChannelDescriptor) -> bool + Send + Sync + 'static>;

// ---------------------------------------------------------------------------
// FrameReader: accumulates bytes from a LiveKit byte stream reader and
// parses successive ws-protocol frames.
// ---------------------------------------------------------------------------

/// Reads chunks from a LiveKit byte stream and parses ws-protocol frames.
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
// ViewerConnection: connects to a LiveKit room and provides helpers for
// reading ws-protocol messages.
// ---------------------------------------------------------------------------

/// A viewer connected to a LiveKit room with an open ws-protocol byte stream.
pub struct ViewerConnection {
    pub room: Room,
    pub events: tokio::sync::mpsc::UnboundedReceiver<RoomEvent>,
    pub frame_reader: FrameReader,
}

impl ViewerConnection {
    /// Connects a viewer to the LiveKit room and waits for the ws-protocol
    /// byte stream to open. Retries the connection if the gateway hasn't
    /// joined the room yet (no ByteStreamOpened within a short window).
    pub async fn connect(room_name: &str, viewer_identity: &str) -> Result<Self> {
        let outer_deadline = tokio::time::Instant::now() + EVENT_TIMEOUT;
        loop {
            let token = livekit_token::generate_token(room_name, viewer_identity)?;
            let (room, mut events) =
                Room::connect(livekit_token::LIVEKIT_URL, &token, RoomOptions::default())
                    .await
                    .context("viewer failed to connect to LiveKit")?;
            info!("{viewer_identity} connected to room, waiting for byte stream");

            // Wait for a ByteStreamOpened event. Use a short inner timeout so we
            // can retry the connection if the gateway hasn't joined yet.
            let inner_deadline = tokio::time::Instant::now() + Duration::from_secs(3);
            let reader = loop {
                let event = tokio::time::timeout_at(inner_deadline, events.recv()).await;
                match event {
                    Err(_) => break None, // inner timeout — retry
                    Ok(None) => anyhow::bail!("room events channel closed"),
                    Ok(Some(RoomEvent::ByteStreamOpened {
                        reader: stream_reader,
                        topic,
                        ..
                    })) if topic == "ws-protocol" => {
                        break Some(stream_reader.take().context("reader already taken")?);
                    }
                    Ok(Some(_)) => continue,
                }
            };

            if let Some(reader) = reader {
                return Ok(Self {
                    room,
                    events,
                    frame_reader: FrameReader::new(reader),
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

    /// Opens a byte stream back to the gateway participant and sends a
    /// binary-framed Subscribe message. Polls `channel.has_sinks()` to confirm
    /// the gateway has processed the subscription.
    pub async fn send_subscribe(&self, channel_ids: &[u64]) -> Result<()> {
        let subscribe = Subscribe::new(channel_ids.iter().copied());
        let inner = subscribe.to_bytes();
        let framed = frame::frame_binary_message(&inner);

        let gateway_identity = ParticipantIdentity(mock_server::TEST_DEVICE_ID.to_string());
        let writer = self
            .room
            .local_participant()
            .stream_bytes(StreamByteOptions {
                topic: "ws-protocol".to_string(),
                destination_identities: vec![gateway_identity],
                ..StreamByteOptions::default()
            })
            .await
            .map_err(|e| anyhow::anyhow!("failed to open byte stream to gateway: {e}"))?;

        writer
            .write(&framed)
            .await
            .map_err(|e| anyhow::anyhow!("failed to write subscribe message: {e}"))?;

        Ok(())
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

    /// Sends a binary-framed Unsubscribe message to the gateway.
    pub async fn send_unsubscribe(&self, channel_ids: &[u64]) -> Result<()> {
        let unsubscribe = Unsubscribe::new(channel_ids.iter().copied());
        let inner = unsubscribe.to_bytes();
        let framed = frame::frame_binary_message(&inner);

        let gateway_identity = ParticipantIdentity(mock_server::TEST_DEVICE_ID.to_string());
        let writer = self
            .room
            .local_participant()
            .stream_bytes(StreamByteOptions {
                topic: "ws-protocol".to_string(),
                destination_identities: vec![gateway_identity],
                ..StreamByteOptions::default()
            })
            .await
            .map_err(|e| anyhow::anyhow!("failed to open byte stream to gateway: {e}"))?;

        writer
            .write(&framed)
            .await
            .map_err(|e| anyhow::anyhow!("failed to write unsubscribe message: {e}"))?;

        Ok(())
    }

    /// Waits for a `TrackSubscribed` room event and returns the track name.
    pub async fn expect_track_subscribed(&mut self) -> Result<String> {
        let deadline = tokio::time::Instant::now() + EVENT_TIMEOUT;
        loop {
            let event = tokio::time::timeout_at(deadline, self.events.recv())
                .await
                .context("timeout waiting for TrackSubscribed event")?
                .context("room events channel closed")?;
            if let RoomEvent::TrackSubscribed { publication, .. } = event {
                return Ok(publication.name());
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
            if let RoomEvent::TrackUnsubscribed { publication, .. } = event {
                return Ok(publication.name());
            }
        }
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
        let room_name = format!("test-room-{}", unique_id());
        let mock = mock_server::start_mock_server(&room_name).await;
        info!("mock server started at {}", mock.url());

        let mut gateway = foxglove::remote_access::Gateway::new()
            .name(format!("test-device-{}", unique_id()))
            .device_token(mock_server::TEST_DEVICE_TOKEN)
            .foxglove_api_url(mock.url())
            .supported_encodings(["json"])
            .context(ctx);

        if let Some(f) = filter {
            gateway = gateway.channel_filter_fn(f);
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
// Utility functions
// ---------------------------------------------------------------------------

/// Polls `cond` until it returns true, or panics after [`EVENT_TIMEOUT`].
pub async fn poll_until(cond: impl Fn() -> bool) {
    let deadline = tokio::time::Instant::now() + EVENT_TIMEOUT;
    while !cond() {
        if tokio::time::Instant::now() >= deadline {
            panic!("poll_until condition not met within {EVENT_TIMEOUT:?}");
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

//! Integration test that validates the ws-protocol byte stream framing and ServerInfo
//! advertisement using a local LiveKit dev server.
//!
//! Requires a local LiveKit server via `docker compose up -d`.
//! Run with: `cargo test -p remote_access_tests -- --ignored livekit_`

use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use livekit::{Room, RoomEvent, RoomOptions};
use remote_access_tests::frame::{self, OpCode};
use remote_access_tests::livekit_token;
use remote_access_tests::mock_server;
use tracing::info;

use foxglove::protocol::v2::server::ServerMessage;

/// Test that a viewer participant receives a correctly-framed ServerInfo message
/// when joining the same LiveKit room as a RemoteAccessSink device.
#[ignore]
#[tokio::test]
async fn livekit_viewer_receives_server_info() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();

    // Use a unique room name to avoid collisions.
    let room_name = format!("test-room-{}", unique_id());

    // Start mock API server that returns LiveKit tokens for the local dev server.
    let mock = mock_server::start_mock_server(&room_name).await;
    info!("mock server started at {}", mock.url());

    // Start RemoteAccessSink pointed at mock API.
    let sink_name = format!("test-device-{}", unique_id());
    let handle = foxglove::RemoteAccessSink::new()
        .name(&sink_name)
        .device_token(mock_server::TEST_DEVICE_TOKEN)
        .foxglove_api_url(mock.url())
        .supported_encodings(["json"])
        .start()
        .context("start RemoteAccessSink")?;

    // Give the SDK time to authenticate and join the LiveKit room.
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Connect as a viewer participant.
    let viewer_token = livekit_token::generate_token(&room_name, "viewer-1")?;
    let (viewer_room, mut viewer_events) = Room::connect(
        livekit_token::LIVEKIT_URL,
        &viewer_token,
        RoomOptions::default(),
    )
    .await
    .context("viewer failed to connect to LiveKit")?;
    info!("viewer connected to room");

    // Wait for a ByteStreamOpened event on the "ws-protocol" topic.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let mut reader = loop {
        let event = tokio::time::timeout_at(deadline, viewer_events.recv())
            .await
            .context("timeout waiting for ByteStreamOpened")?
            .context("room events channel closed")?;

        if let RoomEvent::ByteStreamOpened {
            reader: stream_reader,
            topic,
            participant_identity,
        } = event
        {
            info!("ByteStreamOpened: topic={topic:?}, from={participant_identity:?}");
            if topic == "ws-protocol" {
                break stream_reader.take().context("reader already taken")?;
            }
        }
    };

    // Read chunks and accumulate bytes until we have a complete frame.
    let mut buf = Vec::new();
    let read_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let parsed_frame = loop {
        let chunk = tokio::time::timeout_at(read_deadline, reader.next())
            .await
            .context("timeout reading byte stream chunks")?
            .context("byte stream ended unexpectedly")?
            .map_err(|e| anyhow::anyhow!("stream read error: {e}"))?;

        info!("received {} bytes from byte stream", chunk.len());
        buf.extend_from_slice(&chunk);

        if let Some((frame, _consumed)) = frame::try_parse_frame(&buf)? {
            break frame;
        }
    };

    // Validate frame opcode.
    assert_eq!(
        parsed_frame.op_code,
        OpCode::Text,
        "expected text frame for ServerInfo"
    );

    // Parse the JSON payload as a ServerMessage.
    let json_str = std::str::from_utf8(&parsed_frame.payload).context("invalid UTF-8 payload")?;
    info!("received ServerInfo JSON: {json_str}");

    let msg = ServerMessage::parse_json(json_str).context("failed to parse ServerMessage")?;
    let server_info = match msg {
        ServerMessage::ServerInfo(info) => info,
        other => anyhow::bail!("expected ServerInfo, got: {other:?}"),
    };

    // Validate ServerInfo fields.
    assert_eq!(server_info.name, sink_name, "unexpected server name");
    assert!(
        server_info.session_id.is_some(),
        "session_id should be present"
    );
    assert!(
        server_info.metadata.contains_key("fg-library"),
        "metadata should contain fg-library"
    );
    assert!(
        server_info
            .supported_encodings
            .contains(&"json".to_string()),
        "supported_encodings should contain 'json'"
    );

    info!("ServerInfo validated successfully: {server_info:?}");

    // Cleanup.
    viewer_room
        .close()
        .await
        .context("failed to close viewer room")?;
    let runner = handle.stop();
    tokio::time::timeout(Duration::from_secs(10), runner)
        .await
        .context("timeout waiting for sink to stop")?
        .context("sink runner panicked")?;

    info!("livekit test completed successfully");
    Ok(())
}

/// Generate a unique identifier for use in room names, based on timestamp and PID.
fn unique_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    format!("{nanos:x}-{pid:x}")
}

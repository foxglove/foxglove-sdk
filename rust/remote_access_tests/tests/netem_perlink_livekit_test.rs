//! Two-container per-link netem tests. The gateway and viewer run in separate
//! Docker containers with different IPs so that netem can apply different
//! impairment profiles to each link (simulating a device on a bad network and
//! a viewer on a good network).
//!
//! Each test function is run in its own container:
//!   - `perlink_docker_gateway`: runs in gateway-runner (10.99.0.31, high impairment)
//!   - `perlink_docker_viewer`:  runs in viewer-runner  (10.99.0.40, low impairment)
//!
//! The room name is passed via the `PERLINK_ROOM_NAME` env var (set by the
//! orchestrating script or CI workflow). The viewer signals completion via a
//! done file on the shared `COORDINATION_DIR` tmpfs volume.
//!
//! Run with:
//!   scripts/netem-livekit.sh test perlink

use std::time::Duration;

use anyhow::{Context as _, Result};
use foxglove::protocol::v2::server::ServerMessage;
use remote_access_tests::coordination;
use remote_access_tests::test_helpers::{
    NETEM_EVENT_TIMEOUT, TestGateway, ViewerConnection, poll_until_timeout,
};
use serial_test::serial;
use tracing::info;
use tracing_test::traced_test;

/// Timeout for the gateway to wait for the viewer's done signal.
const GATEWAY_DONE_TIMEOUT: Duration = Duration::from_secs(120);

/// Number of messages in the burst test.
const BURST_COUNT: usize = 20;

/// Gateway side of the two-container per-link test.
///
/// 1. Reads the room name from `PERLINK_ROOM_NAME` env var.
/// 2. Cleans coordination dir.
/// 3. Creates a context, registers a channel, starts the gateway.
/// 4. Waits for the viewer to subscribe (channel gets a sink).
/// 5. Sends a burst of messages.
/// 6. Polls for the done signal from the viewer.
/// 7. Stops the gateway.
#[traced_test]
#[ignore]
#[tokio::test]
// Defensive: these tests run in separate containers, but serial prevents
// accidental concurrent execution if both are run in a single process.
#[serial(perlink_docker)]
async fn perlink_docker_gateway() -> Result<()> {
    let room_name = coordination::room_name()?;
    info!("using room name from env: {room_name}");

    coordination::clean()?;

    let ctx = foxglove::Context::new();
    let channel = ctx
        .channel_builder("/perlink-livekit")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;
    info!("channel created: id={}", channel.id());

    let mock = remote_access_tests::mock_server::start_mock_server(&room_name).await;
    info!("mock server started at {}", mock.url());
    let gw = TestGateway::start_with_mock(&ctx, room_name, mock, None)?;
    info!("gateway started, room={}", gw.room_name);

    // Wait for the viewer to subscribe — the channel gets a sink. Use the
    // netem timeout since the subscribe must traverse the impaired gateway link.
    info!("waiting for viewer to subscribe...");
    poll_until_timeout(|| channel.has_sinks(), NETEM_EVENT_TIMEOUT).await;
    info!("viewer subscribed, sending {BURST_COUNT} messages");

    for i in 0..BURST_COUNT {
        let payload = format!("perlink-msg-{i:04}");
        channel.log(payload.as_bytes());
    }
    info!("all messages sent");

    info!("waiting for viewer done signal (timeout: {GATEWAY_DONE_TIMEOUT:?})...");
    coordination::poll_done(GATEWAY_DONE_TIMEOUT).await?;

    info!("viewer done — stopping gateway");
    gw.stop().await?;
    Ok(())
}

/// Viewer side of the two-container per-link test.
///
/// 1. Reads the room name from `PERLINK_ROOM_NAME` env var.
/// 2. Connects as a viewer to that room.
/// 3. Expects ServerInfo and Advertise.
/// 4. Subscribes and waits for the channel byte stream.
/// 5. Reads the burst of messages and verifies ordering.
/// 6. Writes the done signal.
/// 7. Closes the viewer.
#[traced_test]
#[ignore]
#[tokio::test]
// Defensive: see comment on perlink_docker_gateway.
#[serial(perlink_docker)]
async fn perlink_docker_viewer() -> Result<()> {
    let room_name = coordination::room_name()?;
    info!("using room name from env: {room_name}");

    let mut viewer =
        ViewerConnection::connect_with_timeout(&room_name, "perlink-viewer", NETEM_EVENT_TIMEOUT)
            .await?;
    info!("viewer connected");

    let server_info = viewer.expect_server_info().await?;
    assert!(
        server_info.session_id.is_some(),
        "session_id should be present"
    );
    info!("ServerInfo: {server_info:?}");

    let advertise = viewer.expect_advertise().await?;
    assert!(
        !advertise.channels.is_empty(),
        "expected at least one channel"
    );
    let channel_id = advertise.channels[0].id;
    info!("Advertise: channel_id={channel_id}");

    // Subscribe. We don't have a local channel handle in this container, so
    // send subscribe directly and wait for the channel byte stream to open.
    viewer.send_subscribe(&[channel_id]).await?;
    info!("subscribe sent, waiting for channel byte stream...");

    let mut ch_reader = viewer.expect_channel_byte_stream().await?;
    info!("channel byte stream opened, reading messages...");

    // Read the burst of messages from the gateway.
    for i in 0..BURST_COUNT {
        let msg = ch_reader.next_server_message().await?;
        let expected = format!("perlink-msg-{i:04}");
        match msg {
            ServerMessage::MessageData(data) => {
                assert_eq!(data.channel_id, channel_id);
                assert_eq!(
                    data.data.as_ref(),
                    expected.as_bytes(),
                    "message {i} mismatch"
                );
            }
            other => anyhow::bail!("expected MessageData, got: {other:?}"),
        }
    }
    info!("all {BURST_COUNT} messages received in order");

    coordination::write_done()?;

    viewer.close().await?;
    info!("viewer closed");
    Ok(())
}

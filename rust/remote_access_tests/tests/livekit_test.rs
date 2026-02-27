//! Integration tests that validate the ws-protocol byte stream framing, channel
//! advertisements, subscriptions, and message delivery using a local LiveKit dev server.
//!
//! Requires a local LiveKit server via `docker compose up -d`.
//! Run with: `cargo test -p remote_access_tests -- --ignored livekit_`

use std::time::Duration;

use anyhow::{Context as _, Result};
use foxglove::Schema;
use remote_access_tests::test_helpers::{TestGateway, ViewerConnection, poll_until};
use tracing::info;
use tracing_test::traced_test;

// ===========================================================================
// Tests
// ===========================================================================

/// Test that a viewer participant receives a correctly-framed ServerInfo message
/// when joining the same LiveKit room as a Gateway device.
#[traced_test]
#[ignore]
#[tokio::test]
async fn livekit_viewer_receives_server_info() -> Result<()> {
    let ctx = foxglove::Context::new();
    let gw = TestGateway::start(&ctx).await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let server_info = viewer.expect_server_info().await?;

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
    info!("ServerInfo validated: {server_info:?}");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that when a channel exists before the viewer joins, the viewer receives
/// an Advertise message (after ServerInfo) listing that channel.
#[traced_test]
#[ignore]
#[tokio::test]
async fn livekit_viewer_receives_channel_advertisement() -> Result<()> {
    let ctx = foxglove::Context::new();

    // Create a channel before the viewer joins.
    let channel = ctx
        .channel_builder("/test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;
    info!("created channel id={}", channel.id());

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;

    assert_eq!(advertise.channels.len(), 1, "expected exactly one channel");
    let ch = &advertise.channels[0];
    assert_eq!(ch.topic, "/test");
    assert_eq!(ch.encoding, "json");
    assert_eq!(ch.id, u64::from(channel.id()));
    info!("Advertise validated: channel_id={}", ch.id);

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test the full subscribe-and-receive-data flow: after subscribing to a channel
/// the viewer receives MessageData when the SDK logs to that channel.
#[traced_test]
#[ignore]
#[tokio::test]
async fn livekit_viewer_receives_message_after_subscribe() -> Result<()> {
    let ctx = foxglove::Context::new();
    let channel = ctx
        .channel_builder("/test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
    let channel_id = advertise.channels[0].id;

    // Subscribe to the channel.
    viewer.subscribe_and_wait(&[channel_id], &channel).await?;

    // Log a message.
    let payload = b"hello world";
    channel.log(payload);

    // Expect to receive the message.
    let msg_data = viewer.expect_message_data().await?;
    assert_eq!(msg_data.channel_id, channel_id);
    assert_eq!(msg_data.data.as_ref(), payload);
    info!("MessageData validated: channel_id={channel_id}");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that messages logged before the viewer subscribes are not delivered.
#[traced_test]
#[ignore]
#[tokio::test]
async fn livekit_viewer_does_not_receive_message_before_subscribe() -> Result<()> {
    let ctx = foxglove::Context::new();
    let channel = ctx
        .channel_builder("/test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
    let channel_id = advertise.channels[0].id;

    // Log a message BEFORE subscribing — this should NOT be delivered.
    channel.log(b"message-before-subscribe");

    // Now subscribe.
    viewer.subscribe_and_wait(&[channel_id], &channel).await?;

    // Log a second message — this one should be delivered.
    let expected_payload = b"message-after-subscribe";
    channel.log(expected_payload);

    let msg_data = viewer.expect_message_data().await?;
    assert_eq!(msg_data.channel_id, channel_id);
    assert_eq!(
        msg_data.data.as_ref(),
        expected_payload,
        "should only receive the message logged after subscribing"
    );
    info!("subscription gating validated");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that when a channel is closed, the viewer receives an Unadvertise message.
#[traced_test]
#[ignore]
#[tokio::test]
async fn livekit_viewer_receives_unadvertise_on_channel_close() -> Result<()> {
    let ctx = foxglove::Context::new();
    let channel = ctx
        .channel_builder("/test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
    let channel_id = advertise.channels[0].id;

    // Close the channel.
    channel.close();

    let unadvertise = viewer.expect_unadvertise().await?;
    assert_eq!(unadvertise.channel_ids, vec![channel_id]);
    info!("Unadvertise validated: channel_id={channel_id}");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that channels created after the viewer has connected are still advertised.
#[traced_test]
#[ignore]
#[tokio::test]
async fn livekit_viewer_receives_advertisement_for_late_channel() -> Result<()> {
    let ctx = foxglove::Context::new();

    // Start gateway with NO channels.
    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;

    // Now create a channel after the viewer is connected.
    let channel = ctx
        .channel_builder("/late-topic")
        .message_encoding("json")
        .build_raw()
        .context("create late channel")?;

    let advertise = viewer.expect_advertise().await?;
    assert_eq!(advertise.channels.len(), 1);
    assert_eq!(advertise.channels[0].topic, "/late-topic");
    assert_eq!(advertise.channels[0].id, u64::from(channel.id()));
    info!("late channel advertisement validated");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that channels excluded by the Gateway's channel_filter_fn are not
/// advertised to viewers.
#[traced_test]
#[ignore]
#[tokio::test]
async fn livekit_channel_filter_excludes_filtered_channels() -> Result<()> {
    let ctx = foxglove::Context::new();

    // Create two channels: one allowed, one blocked.
    let allowed = ctx
        .channel_builder("/allowed/data")
        .message_encoding("json")
        .build_raw()
        .context("create allowed channel")?;
    let _blocked = ctx
        .channel_builder("/blocked/data")
        .message_encoding("json")
        .build_raw()
        .context("create blocked channel")?;

    // Start gateway with a filter that only allows topics starting with "/allowed".
    let gw = TestGateway::start_with_filter(
        &ctx,
        Some(Box::new(|ch: &foxglove::ChannelDescriptor| {
            ch.topic().starts_with("/allowed")
        })),
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;

    assert_eq!(
        advertise.channels.len(),
        1,
        "only the allowed channel should be advertised"
    );
    assert_eq!(advertise.channels[0].topic, "/allowed/data");
    assert_eq!(advertise.channels[0].id, u64::from(allowed.id()));
    info!("channel filter validated");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that message delivery works correctly across multiple participants.
#[traced_test]
#[ignore]
#[tokio::test]
async fn livekit_multiple_participants_receive_messages() -> Result<()> {
    let ctx = foxglove::Context::new();
    let channel = ctx
        .channel_builder("/test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start(&ctx).await?;

    // Connect viewer-1, subscribe.
    let mut viewer1 = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _si1 = viewer1.expect_server_info().await?;
    let adv1 = viewer1.expect_advertise().await?;
    let channel_id = adv1.channels[0].id;
    viewer1.subscribe_and_wait(&[channel_id], &channel).await?;

    // Log message-1 — only viewer-1 should receive it.
    channel.log(b"message-1");
    let msg1 = viewer1.expect_message_data().await?;
    assert_eq!(msg1.data.as_ref(), b"message-1");
    info!("viewer-1 received message-1");

    // Connect viewer-2, subscribe.
    let mut viewer2 = ViewerConnection::connect(&gw.room_name, "viewer-2").await?;
    let _si2 = viewer2.expect_server_info().await?;
    let adv2 = viewer2.expect_advertise().await?;
    assert_eq!(adv2.channels[0].id, channel_id);
    viewer2.send_subscribe(&[channel_id]).await?;
    // Channel already has a sink from viewer-1, so we can't poll has_sinks().
    // Use a brief settle time for the gateway to process viewer-2's subscription.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Log message-2 — both viewers should receive it.
    channel.log(b"message-2");

    let msg2_v1 = viewer1.expect_message_data().await?;
    assert_eq!(msg2_v1.data.as_ref(), b"message-2");
    info!("viewer-1 received message-2");

    let msg2_v2 = viewer2.expect_message_data().await?;
    assert_eq!(msg2_v2.data.as_ref(), b"message-2");
    info!("viewer-2 received message-2");

    // Disconnect viewer-1.
    viewer1.close().await?;
    // Allow time for the gateway to process the disconnection.
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Log message-3 — only viewer-2 should receive it.
    channel.log(b"message-3");
    let msg3_v2 = viewer2.expect_message_data().await?;
    assert_eq!(msg3_v2.data.as_ref(), b"message-3");
    info!("viewer-2 received message-3 (viewer-1 disconnected)");

    viewer2.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that a video-capable channel (protobuf-encoded foxglove.RawImage) is advertised
/// with `foxglove.hasVideoTrack` metadata.
#[traced_test]
#[ignore]
#[tokio::test]
async fn livekit_video_channel_has_video_track_metadata() -> Result<()> {
    let ctx = foxglove::Context::new();

    // Create a video-capable channel and a plain JSON channel.
    let video_channel = ctx
        .channel_builder("/camera")
        .message_encoding("protobuf")
        .schema(Schema::new("foxglove.RawImage", "protobuf", &b""[..]))
        .build_raw()
        .context("create video channel")?;
    let json_channel = ctx
        .channel_builder("/data")
        .message_encoding("json")
        .build_raw()
        .context("create json channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;

    assert_eq!(advertise.channels.len(), 2);
    for ch in &advertise.channels {
        if ch.id == u64::from(video_channel.id()) {
            assert_eq!(
                ch.metadata
                    .get("foxglove.hasVideoTrack")
                    .map(|s| s.as_str()),
                Some("true"),
                "video channel should have foxglove.hasVideoTrack metadata"
            );
        } else {
            assert_eq!(ch.id, u64::from(json_channel.id()));
            assert!(
                !ch.metadata.contains_key("foxglove.hasVideoTrack"),
                "json channel should not have foxglove.hasVideoTrack metadata"
            );
        }
    }
    info!("video track metadata validated");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that messages logged to a video-capable channel are routed through the video
/// publisher and do NOT produce MessageData frames on the data plane.
#[traced_test]
#[ignore]
#[tokio::test]
async fn livekit_video_channel_messages_bypass_data_plane() -> Result<()> {
    let ctx = foxglove::Context::new();

    let video_channel = ctx
        .channel_builder("/camera")
        .message_encoding("protobuf")
        .schema(Schema::new("foxglove.RawImage", "protobuf", &b""[..]))
        .build_raw()
        .context("create video channel")?;
    let json_channel = ctx
        .channel_builder("/data")
        .message_encoding("json")
        .build_raw()
        .context("create json channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let _advertise = viewer.expect_advertise().await?;

    let video_id = u64::from(video_channel.id());
    let json_id = u64::from(json_channel.id());

    // Subscribe to both channels.
    viewer.send_subscribe(&[video_id, json_id]).await?;
    poll_until(|| json_channel.has_sinks()).await;

    // Log to the video channel first, then the JSON channel.
    // If the video message leaked to the data plane, it would arrive before
    // the JSON message (FIFO ordering).
    video_channel.log(b"video-frame");
    json_channel.log(b"json-payload");

    let msg = viewer.expect_message_data().await?;
    assert_eq!(msg.channel_id, json_id, "should receive the JSON message");
    assert_eq!(msg.data.as_ref(), b"json-payload");
    info!("video channel correctly bypassed data plane");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that subscribing to a video-capable channel publishes a video track to the
/// LiveKit room, and unsubscribing tears it down.
#[traced_test]
#[ignore]
#[tokio::test]
async fn livekit_video_track_lifecycle() -> Result<()> {
    let ctx = foxglove::Context::new();

    let video_channel = ctx
        .channel_builder("/camera")
        .message_encoding("protobuf")
        .schema(Schema::new("foxglove.RawImage", "protobuf", &b""[..]))
        .build_raw()
        .context("create video channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
    let channel_id = advertise.channels[0].id;

    // Subscribe to the video channel — the gateway should publish a video track.
    viewer
        .subscribe_and_wait(&[channel_id], &video_channel)
        .await?;
    let track_name = viewer.expect_track_subscribed().await?;
    assert_eq!(track_name, "/camera", "video track name should match topic");
    info!("video track published on subscribe: {track_name}");

    // Unsubscribe — the gateway should unpublish the video track.
    viewer.send_unsubscribe(&[channel_id]).await?;
    let track_name = viewer.expect_track_unsubscribed().await?;
    assert_eq!(track_name, "/camera");
    info!("video track torn down on unsubscribe: {track_name}");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that a video track can be re-established after an unsubscribe/resubscribe cycle.
/// Validates that the video schema persists across teardown so the track can be recreated.
#[traced_test]
#[ignore]
#[tokio::test]
async fn livekit_video_track_resubscribe() -> Result<()> {
    let ctx = foxglove::Context::new();

    let video_channel = ctx
        .channel_builder("/camera")
        .message_encoding("protobuf")
        .schema(Schema::new("foxglove.RawImage", "protobuf", &b""[..]))
        .build_raw()
        .context("create video channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
    let channel_id = advertise.channels[0].id;

    // First subscribe — video track should be published.
    viewer
        .subscribe_and_wait(&[channel_id], &video_channel)
        .await?;
    let track_name = viewer.expect_track_subscribed().await?;
    assert_eq!(track_name, "/camera");
    info!("first subscribe: video track published");

    // Unsubscribe — video track should be torn down.
    viewer.send_unsubscribe(&[channel_id]).await?;
    let track_name = viewer.expect_track_unsubscribed().await?;
    assert_eq!(track_name, "/camera");
    info!("unsubscribe: video track torn down");

    // Resubscribe — video track should come back.
    viewer.send_subscribe(&[channel_id]).await?;
    let track_name = viewer.expect_track_subscribed().await?;
    assert_eq!(track_name, "/camera");
    info!("resubscribe: video track re-established");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

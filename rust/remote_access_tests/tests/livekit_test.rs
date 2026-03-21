//! Integration tests that validate the ws-protocol byte stream framing, channel
//! advertisements, subscriptions, and message delivery using a local LiveKit dev server.
//!
//! Requires a local LiveKit server via `docker compose up -d`.
//! Run with: `cargo test -p remote_access_tests -- --ignored livekit_`

use std::time::Duration;

use anyhow::{Context as _, Result};
use foxglove::messages::{RawImage, Timestamp};
use foxglove::protocol::v2::client::SubscribeChannel;
use foxglove::protocol::v2::server::ServerMessage;
use foxglove::{Encode, Schema};
use livekit::{Room, RoomOptions};
use remote_access_tests::livekit_token;
use remote_access_tests::mock_listener::MockListener;
use remote_access_tests::test_helpers::{
    ClientChannelDesc, EVENT_TIMEOUT, TestGateway, TestGatewayOptions, ViewerConnection, poll_until,
};
use serial_test::serial;
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
#[serial(livekit)]
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
#[serial(livekit)]
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
#[serial(livekit)]
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
    let msg_data = viewer.expect_new_bytestream_and_message_data().await?;
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
#[serial(livekit)]
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

    let msg_data = viewer.expect_new_bytestream_and_message_data().await?;
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
#[serial(livekit)]
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
#[serial(livekit)]
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
#[serial(livekit)]
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
#[serial(livekit)]
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
    // Connect viewer-2
    let mut viewer2 = ViewerConnection::connect(&gw.room_name, "viewer-2").await?;
    let _si1 = viewer1.expect_server_info().await?;
    let adv1 = viewer1.expect_advertise().await?;
    let channel_id = adv1.channels[0].id;
    viewer1.subscribe_and_wait(&[channel_id], &channel).await?;

    // Log message-1 — only viewer-1 should receive it.
    channel.log(b"message-1");
    let msg1 = viewer1.expect_new_bytestream_and_message_data().await?;
    assert_eq!(msg1.data.as_ref(), b"message-1");
    info!("viewer-1 received message-1");
    // viewer-2 won't receive message-1, but we verify that below when it reads message-2 as expected and not message-1

    // Subscribe viewer-2
    let _si2 = viewer2.expect_server_info().await?;
    let adv2 = viewer2.expect_advertise().await?;
    assert_eq!(adv2.channels[0].id, channel_id);
    viewer2.send_subscribe(&[channel_id]).await?;
    // Channel already has a sink from viewer-1, so we can't poll has_sinks().
    // Use a brief settle time for the gateway to process viewer-2's subscription.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Log message-2 — both viewers should receive it.
    channel.log(b"message-2");

    let msg2_v1 = viewer1.expect_new_bytestream_and_message_data().await?;
    assert_eq!(msg2_v1.data.as_ref(), b"message-2");
    info!("viewer-1 received message-2");

    let msg2_v2 = viewer2.expect_new_bytestream_and_message_data().await?;
    assert_eq!(msg2_v2.data.as_ref(), b"message-2");
    info!("viewer-2 received message-2");

    // Disconnect viewer-1.
    viewer1.close().await?;
    // Wait until viewer-2 sees the disconnect (confirming the gateway has also received
    // the ParticipantDisconnected event), then allow a brief settle for the gateway to
    // update its subscription state before we log the next message.
    viewer2
        .wait_for_participant_disconnected("viewer-1")
        .await?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Log message-3 — only viewer-2 should receive it.
    channel.log(b"message-3");
    let msg3_v2 = viewer2.expect_new_bytestream_and_message_data().await?;
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
#[serial(livekit)]
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
#[serial(livekit)]
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

    // Subscribe to both channels: video with requestVideoTrack, json without.
    viewer
        .send_subscribe_channels(vec![
            SubscribeChannel {
                id: video_id,
                request_video_track: true,
            },
            SubscribeChannel {
                id: json_id,
                request_video_track: false,
            },
        ])
        .await?;
    poll_until(|| json_channel.has_sinks()).await;

    // Log to the video channel first, then the JSON channel.
    // If the video message leaked to the data plane, it would arrive before
    // the JSON message (FIFO ordering).
    video_channel.log(b"video-frame");
    json_channel.log(b"json-payload");

    let msg = viewer.expect_new_bytestream_and_message_data().await?;
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
#[serial(livekit)]
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

    // Subscribe to the video channel with requestVideoTrack — the gateway should publish a video track.
    viewer
        .subscribe_video_and_wait(&[channel_id], &video_channel)
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
#[serial(livekit)]
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

    // First subscribe with requestVideoTrack — video track should be published.
    viewer
        .subscribe_video_and_wait(&[channel_id], &video_channel)
        .await?;
    let track_name = viewer.expect_track_subscribed().await?;
    assert_eq!(track_name, "/camera");
    info!("first subscribe: video track published");

    // Unsubscribe — video track should be torn down.
    viewer.send_unsubscribe(&[channel_id]).await?;
    let track_name = viewer.expect_track_unsubscribed().await?;
    assert_eq!(track_name, "/camera");
    info!("unsubscribe: video track torn down");

    // Resubscribe with requestVideoTrack — video track should come back.
    viewer
        .send_subscribe_channels(vec![SubscribeChannel {
            id: channel_id,
            request_video_track: true,
        }])
        .await?;
    let track_name = viewer.expect_track_subscribed().await?;
    assert_eq!(track_name, "/camera");
    info!("resubscribe: video track re-established");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that when a viewer is already in the room before the gateway joins,
/// the viewer still receives ServerInfo and Advertise messages.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_existing_participant_receives_server_info_and_advertisement() -> Result<()> {
    let ctx = foxglove::Context::new();
    let channel = ctx
        .channel_builder("/test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    // Create mock server and room name without starting the gateway yet.
    let (room_name, mock) = TestGateway::prepare().await;

    // Connect viewer to the room BEFORE the gateway joins.
    let token = livekit_token::generate_token(&room_name, "viewer-1")?;
    let (room, events) = Room::connect(
        &livekit_token::livekit_url(),
        &token,
        RoomOptions::default(),
    )
    .await
    .context("viewer failed to connect to LiveKit")?;
    info!("viewer connected to room before gateway");

    // Now start the gateway — it should discover the existing viewer participant.
    let gw = TestGateway::start_with_mock(&ctx, room_name, mock, Default::default())?;

    // Wait for the gateway to open a byte stream to the viewer.
    let mut viewer = ViewerConnection::from_room(room, events).await?;

    // Verify the viewer receives ServerInfo and Advertise.
    let server_info = viewer.expect_server_info().await?;
    assert!(
        server_info.session_id.is_some(),
        "session_id should be present"
    );
    assert!(
        server_info.metadata.contains_key("fg-library"),
        "metadata should contain fg-library"
    );

    let advertise = viewer.expect_advertise().await?;
    assert_eq!(advertise.channels.len(), 1, "expected exactly one channel");
    assert_eq!(advertise.channels[0].topic, "/test");
    assert_eq!(advertise.channels[0].id, u64::from(channel.id()));
    info!("existing participant received server info and advertisement");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that subscribing to a video-capable channel WITHOUT requestVideoTrack delivers
/// data via the ws-protocol data plane (MessageData frames) instead of a video track.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_video_channel_without_request_video_track_uses_data_plane() -> Result<()> {
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

    // Subscribe without requestVideoTrack — data should come via data plane.
    viewer
        .subscribe_and_wait(&[channel_id], &video_channel)
        .await?;

    video_channel.log(b"video-frame");
    let msg = viewer.expect_new_bytestream_and_message_data().await?;
    assert_eq!(msg.channel_id, channel_id);
    assert_eq!(msg.data.as_ref(), b"video-frame");
    info!("video data received via data plane (no video track requested)");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that re-subscribing with requestVideoTrack: false after previously subscribing
/// with requestVideoTrack: true tears down the video track and switches to data plane delivery.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_video_resubscribe_switches_to_data_plane() -> Result<()> {
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

    // First subscribe with requestVideoTrack: true — video track should be published.
    viewer
        .subscribe_video_and_wait(&[channel_id], &video_channel)
        .await?;
    let track_name = viewer.expect_track_subscribed().await?;
    assert_eq!(track_name, "/camera");
    info!("video track published");

    // Re-subscribe with requestVideoTrack: false — video track should be torn down.
    viewer
        .send_subscribe_channels(vec![SubscribeChannel {
            id: channel_id,
            request_video_track: false,
        }])
        .await?;
    let track_name = viewer.expect_track_unsubscribed().await?;
    assert_eq!(track_name, "/camera");
    info!("video track torn down after re-subscribe with requestVideoTrack: false");

    // Data should now arrive via the data plane.
    video_channel.log(b"video-frame");
    let msg = viewer.expect_new_bytestream_and_message_data().await?;
    assert_eq!(msg.channel_id, channel_id);
    assert_eq!(msg.data.as_ref(), b"video-frame");
    info!("data received via data plane after switching from video");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that requesting a video track on a non-video-capable channel sends an error status
/// message and drops the subscription.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_request_video_track_on_non_video_channel_sends_error() -> Result<()> {
    let ctx = foxglove::Context::new();

    let json_channel = ctx
        .channel_builder("/json_data")
        .message_encoding("json")
        .build_raw()
        .context("create json channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
    let channel_id = advertise.channels[0].id;

    // Subscribe with requestVideoTrack on a channel that doesn't support video.
    viewer
        .send_subscribe_channels(vec![SubscribeChannel {
            id: channel_id,
            request_video_track: true,
        }])
        .await?;

    // Expect an error status message.
    let status = viewer.expect_status().await?;
    assert_eq!(
        status.level,
        foxglove::protocol::v2::server::status::Level::Error
    );
    assert!(
        status
            .message
            .contains("does not support video transcoding"),
        "unexpected status message: {}",
        status.message
    );
    info!("received error status: {}", status.message);

    // The subscription should have been dropped — channel should have no sinks.
    assert!(
        !json_channel.has_sinks(),
        "channel should have no sinks after rejected video subscription"
    );

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Encode a 4x4 rgb8 `foxglove.RawImage` as protobuf bytes.
fn encode_raw_image(frame_id: &str) -> Vec<u8> {
    let width: u32 = 4;
    let height: u32 = 4;
    let step = width * 3; // rgb8: 3 bytes per pixel
    let data = vec![128u8; (step * height) as usize];
    let msg = RawImage {
        timestamp: Some(Timestamp::new(1, 0)),
        frame_id: frame_id.to_string(),
        width,
        height,
        encoding: "rgb8".to_string(),
        step,
        data: data.into(),
    };
    let mut buf = Vec::new();
    Encode::encode(&msg, &mut buf).expect("encode RawImage");
    buf
}

/// Test that logging a valid image to a video channel causes a re-advertisement
/// with `foxglove.videoSourceEncoding` and `foxglove.videoFrameId` metadata.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_video_metadata_advertised_after_image_logged() -> Result<()> {
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

    // Initial advertisement should NOT have video metadata yet.
    assert!(
        !advertise.channels[0]
            .metadata
            .contains_key("foxglove.videoSourceEncoding"),
        "initial advertisement should not have videoSourceEncoding"
    );

    // Subscribe with video track requested.
    viewer
        .subscribe_video_and_wait(&[channel_id], &video_channel)
        .await?;
    let _track_name = viewer.expect_track_subscribed().await?;

    // Log a valid protobuf-encoded RawImage.
    let image_bytes = encode_raw_image("camera_optical_frame");
    video_channel.log(&image_bytes);

    // Wait for a re-advertisement that includes the video metadata.
    // The session's run_sender loop will detect the metadata change and re-advertise.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let metadata = loop {
        let msg = tokio::time::timeout_at(deadline, viewer.frame_reader.next_server_message())
            .await
            .context("timeout waiting for re-advertisement with video metadata")?
            .context("failed to read server message")?;

        if let ServerMessage::Advertise(adv) = msg {
            if let Some(ch) = adv.channels.iter().find(|c| c.id == channel_id) {
                if ch.metadata.contains_key("foxglove.videoSourceEncoding") {
                    break ch.metadata.clone();
                }
            }
        }
        // Otherwise keep reading (might receive other messages).
    };

    assert_eq!(
        metadata
            .get("foxglove.videoSourceEncoding")
            .map(|s| s.as_str()),
        Some("rgb8"),
        "videoSourceEncoding should be rgb8"
    );
    assert_eq!(
        metadata.get("foxglove.videoFrameId").map(|s| s.as_str()),
        Some("camera_optical_frame"),
        "videoFrameId should be camera_optical_frame"
    );
    assert_eq!(
        metadata.get("foxglove.hasVideoTrack").map(|s| s.as_str()),
        Some("true"),
        "hasVideoTrack should still be present"
    );
    info!("video metadata re-advertisement validated: {metadata:?}");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that when a viewer sends a client Advertise message the gateway fires
/// `on_client_advertise` on the listener with the correct client identity and topic.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_client_advertise_fires_listener_callback() -> Result<()> {
    use std::sync::Arc;
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            capabilities: vec![foxglove::remote_access::Capability::ClientPublish],
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.frame_reader.next_server_message().await?;

    // Send a client Advertise for one channel.
    viewer
        .send_client_advertise(&[ClientChannelDesc {
            id: 1,
            topic: "/cmd".to_string(),
            encoding: "json".to_string(),
        }])
        .await?;

    // Wait for the listener callback to fire.
    poll_until(|| listener.advertised().len() == 1).await;

    let advertised = listener.advertised();
    assert_eq!(advertised.len(), 1);
    assert_eq!(
        advertised[0].0, "viewer-1",
        "client id should be viewer identity"
    );
    assert_eq!(
        advertised[0].1, "/cmd",
        "topic should match advertised channel"
    );
    info!(
        "on_client_advertise callback validated: {:?}",
        advertised[0]
    );

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that a client Unadvertise message fires `on_client_unadvertise` on the listener.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_client_unadvertise_fires_listener_callback() -> Result<()> {
    use std::sync::Arc;
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            capabilities: vec![foxglove::remote_access::Capability::ClientPublish],
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.frame_reader.next_server_message().await?;

    // Advertise a channel first.
    viewer
        .send_client_advertise(&[ClientChannelDesc {
            id: 42,
            topic: "/joy".to_string(),
            encoding: "json".to_string(),
        }])
        .await?;
    poll_until(|| listener.advertised().len() == 1).await;

    // Now unadvertise it.
    viewer.send_client_unadvertise(&[42]).await?;
    poll_until(|| listener.unadvertised().len() == 1).await;

    let unadvertised = listener.unadvertised();
    assert_eq!(unadvertised.len(), 1);
    assert_eq!(unadvertised[0].0, "viewer-1");
    assert_eq!(unadvertised[0].1, "/joy");
    info!(
        "on_client_unadvertise callback validated: {:?}",
        unadvertised[0]
    );

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that when a viewer disconnects, `on_client_unadvertise` fires for all channels
/// the viewer had advertised.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_client_disconnect_fires_unadvertise_for_advertised_channels() -> Result<()> {
    use std::sync::Arc;
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            capabilities: vec![foxglove::remote_access::Capability::ClientPublish],
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.frame_reader.next_server_message().await?;

    // Advertise two channels.
    viewer
        .send_client_advertise(&[
            ClientChannelDesc {
                id: 1,
                topic: "/cmd_vel".to_string(),
                encoding: "json".to_string(),
            },
            ClientChannelDesc {
                id: 2,
                topic: "/joy".to_string(),
                encoding: "json".to_string(),
            },
        ])
        .await?;
    poll_until(|| listener.advertised().len() == 2).await;

    // Disconnect the viewer — the gateway should fire on_client_unadvertise for both channels.
    viewer.close().await?;
    poll_until(|| listener.unadvertised().len() == 2).await;

    let unadvertised = listener.unadvertised();
    assert_eq!(unadvertised.len(), 2);
    let topics: Vec<&str> = unadvertised.iter().map(|(_, t)| t.as_str()).collect();
    assert!(
        topics.contains(&"/cmd_vel"),
        "expected /cmd_vel in unadvertised: {topics:?}"
    );
    assert!(
        topics.contains(&"/joy"),
        "expected /joy in unadvertised: {topics:?}"
    );
    info!("disconnect unadvertise validated: {unadvertised:?}");

    gw.stop().await?;
    Ok(())
}

// ===========================================================================
// Client publish / message data tests
// ===========================================================================

/// Test that sending a client MessageData on the client-{channelId} stream
/// fires `on_message_data` on the listener with the correct client, topic, and payload.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_client_message_data_fires_listener_callback() -> Result<()> {
    use std::sync::Arc;
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            capabilities: vec![foxglove::remote_access::Capability::ClientPublish],
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    viewer
        .send_client_advertise(&[ClientChannelDesc {
            id: 1,
            topic: "/cmd".to_string(),
            encoding: "json".to_string(),
        }])
        .await?;
    poll_until(|| listener.advertised().len() == 1).await;

    let payload = b"{\"velocity\": 1.0}";
    viewer.send_client_message_data(1, payload).await?;

    poll_until(|| listener.message_data().len() == 1).await;

    let messages = listener.message_data();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].0, "viewer-1", "client id should match");
    assert_eq!(messages[0].1, "/cmd", "topic should match");
    assert_eq!(messages[0].2, payload, "payload should match");
    info!("on_message_data callback validated via ws-protocol stream");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that sending MessageData before the Client Advertise still delivers the
/// message once the advertise arrives (the server holds the byte stream until then).
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_client_message_data_before_advertise_is_delivered() -> Result<()> {
    use std::sync::Arc;
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            capabilities: vec![foxglove::remote_access::Capability::ClientPublish],
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    let payload = b"early data";
    viewer.send_client_message_data(1, payload).await?;

    // Brief pause to make it likely the data stream arrives before the advertise.
    tokio::time::sleep(Duration::from_millis(500)).await;

    viewer
        .send_client_advertise(&[ClientChannelDesc {
            id: 1,
            topic: "/cmd".to_string(),
            encoding: "json".to_string(),
        }])
        .await?;

    poll_until(|| listener.message_data().len() == 1).await;

    let messages = listener.message_data();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].0, "viewer-1", "client id should match");
    assert_eq!(messages[0].1, "/cmd", "topic should match");
    assert_eq!(messages[0].2, payload, "payload should match");
    info!("message data delivered after late advertise");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that sending client advertisement when the gateway does not have the `ClientPublish`
/// capability results in a Status error message.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_client_message_advertise_without_capability_sends_error() -> Result<()> {
    let ctx = foxglove::Context::new();

    let gw = TestGateway::start(&ctx).await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    viewer
        .send_client_advertise(&[ClientChannelDesc {
            id: 1,
            topic: "/cmd".to_string(),
            encoding: "json".to_string(),
        }])
        .await?;

    let deadline = tokio::time::Instant::now() + EVENT_TIMEOUT;
    let status = loop {
        let msg = tokio::time::timeout_at(deadline, viewer.frame_reader.next_server_message())
            .await
            .context("timeout waiting for error status")?
            .context("failed to read server message")?;
        if let ServerMessage::Status(s) = msg {
            break s;
        }
    };

    assert_eq!(
        status.level,
        foxglove::protocol::v2::server::status::Level::Error
    );
    info!("error status received: {}", status.message);

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

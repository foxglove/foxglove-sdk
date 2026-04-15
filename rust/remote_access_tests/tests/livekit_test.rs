//! Integration tests that validate the byte stream framing, channel
//! advertisements, subscriptions, and message delivery using a local LiveKit dev server.
//!
//! Requires a local LiveKit server via `docker compose up -d`.
//! Run with: `cargo test -p remote_access_tests -- --ignored livekit_`

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context as _, Result};
use foxglove::messages::{RawImage, Timestamp};
use foxglove::protocol::v2::client::SubscribeChannel;
use foxglove::protocol::v2::server::ServerMessage;
use foxglove::remote_access::{ConnectionGraph, ConnectionStatus};
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
/// the viewer receives multiple sequential MessageData frames on the same
/// per-channel byte stream.
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

    // Wait for the device data track to be published and subscribe to it.
    let mut ch_reader = viewer.expect_device_channel_data_track(channel_id).await?;

    let payloads: &[&[u8]] = &[b"message-1", b"message-2", b"message-3"];

    for (i, &payload) in payloads.iter().enumerate() {
        channel.log(payload);
        let msg = ch_reader.next_message_data().await?;
        assert_eq!(msg.data.as_ref(), payload);
        info!("received message {}/{}", i + 1, payloads.len());
    }

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

    // Now subscribe and wait for the data track to be ready.
    viewer.subscribe_and_wait(&[channel_id], &channel).await?;
    viewer.ensure_device_data_track(channel_id).await?;

    // Log a second message — this one should be delivered.
    let expected_payload = b"message-after-subscribe";
    channel.log(expected_payload);

    let msg_data = viewer
        .expect_new_data_track_and_message_data(channel_id)
        .await?;
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

    // Wait for the device data track and subscribe to it.
    let mut ch_reader1 = viewer1.expect_device_channel_data_track(channel_id).await?;

    // Log message-1 — only viewer-1 should receive it.
    channel.log(b"message-1");
    let msg1 = ch_reader1.next_message_data().await?;
    assert_eq!(msg1.data.as_ref(), b"message-1");
    info!("viewer-1 received message-1");

    // Subscribe viewer-2
    let _si2 = viewer2.expect_server_info().await?;
    let adv2 = viewer2.expect_advertise().await?;
    assert_eq!(adv2.channels[0].id, channel_id);
    viewer2.send_subscribe(&[channel_id]).await?;
    // Wait for viewer-2 to receive and subscribe to the device data track.
    let mut ch_reader2 = viewer2.expect_device_channel_data_track(channel_id).await?;
    // Brief settle for the gateway to process viewer-2's subscription.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Log message-2 — both viewers should receive it.
    channel.log(b"message-2");

    let msg2_v1 = ch_reader1.next_message_data().await?;
    assert_eq!(msg2_v1.data.as_ref(), b"message-2");
    info!("viewer-1 received message-2");

    let msg2_v2 = ch_reader2.next_message_data().await?;
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
    let msg3_v2 = ch_reader2.next_message_data().await?;
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

    // Wait for the JSON channel's data track to be ready.
    viewer.ensure_device_data_track(json_id).await?;

    // Log to the video channel first, then the JSON channel.
    // If the video message leaked to the data plane, it would arrive before
    // the JSON message (FIFO ordering).
    video_channel.log(b"video-frame");
    json_channel.log(b"json-payload");

    let msg = viewer
        .expect_new_data_track_and_message_data(json_id)
        .await?;
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
/// data via the device-ch data plane (MessageData frames) instead of a video track.
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
    viewer.ensure_device_data_track(channel_id).await?;

    video_channel.log(b"video-frame");
    let msg = viewer
        .expect_new_data_track_and_message_data(channel_id)
        .await?;
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

    // Wait for the device data track to be published and subscribe.
    viewer.ensure_device_data_track(channel_id).await?;

    // Data should now arrive via the data plane.
    video_channel.log(b"video-frame");
    let msg = viewer
        .expect_new_data_track_and_message_data(channel_id)
        .await?;
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
            schema_name: String::new(),
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

/// Test that a client Advertise with a schema_name but no binary schema data still
/// preserves the schema_name on the ChannelDescriptor delivered to the listener.
/// This is the typical case for teleop panels (e.g. publishing to /cmd_vel).
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_client_advertise_preserves_schema_name_without_schema_data() -> Result<()> {
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

    // Advertise a channel with schema_name but no schema data — this is what the
    // Foxglove teleop panel sends for /cmd_vel.
    viewer
        .send_client_advertise(&[ClientChannelDesc {
            id: 1,
            topic: "/cmd_vel".to_string(),
            encoding: "json".to_string(),
            schema_name: "geometry_msgs/msg/Twist".to_string(),
        }])
        .await?;

    poll_until(|| listener.advertised().len() == 1).await;

    let advertised = listener.advertised();
    assert_eq!(advertised.len(), 1);
    assert_eq!(advertised[0].0, "viewer-1");
    assert_eq!(advertised[0].1, "/cmd_vel");
    assert_eq!(
        advertised[0].2, "geometry_msgs/msg/Twist",
        "schema_name should be preserved even without binary schema data"
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
            schema_name: String::new(),
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
                schema_name: String::new(),
            },
            ClientChannelDesc {
                id: 2,
                topic: "/joy".to_string(),
                encoding: "json".to_string(),
                schema_name: String::new(),
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
// Subscribe / unsubscribe listener callback tests
// ===========================================================================

/// Test that subscribing to a channel fires `on_subscribe` on the listener.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_subscribe_fires_listener_callback() -> Result<()> {
    use std::sync::Arc;
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let channel = ctx
        .channel_builder("/camera")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
    let channel_id = advertise.channels[0].id;

    viewer.send_subscribe(&[channel_id]).await?;
    poll_until(|| listener.subscribed().len() == 1).await;

    let subscribed = listener.subscribed();
    assert_eq!(subscribed.len(), 1);
    assert_eq!(subscribed[0].0, "viewer-1");
    assert_eq!(subscribed[0].1, "/camera");
    info!("on_subscribe callback validated: {:?}", subscribed[0]);

    viewer.close().await?;
    // Wait for disconnect unsubscribe before stopping.
    poll_until(|| listener.unsubscribed().len() == 1).await;
    let _ = channel;
    gw.stop().await?;
    Ok(())
}

/// Test that unsubscribing from a channel fires `on_unsubscribe` on the listener.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_unsubscribe_fires_listener_callback() -> Result<()> {
    use std::sync::Arc;
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let channel = ctx
        .channel_builder("/lidar")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
    let channel_id = advertise.channels[0].id;

    // Subscribe first, then unsubscribe.
    viewer.subscribe_and_wait(&[channel_id], &channel).await?;
    poll_until(|| listener.subscribed().len() == 1).await;

    viewer.send_unsubscribe(&[channel_id]).await?;
    poll_until(|| listener.unsubscribed().len() == 1).await;

    let unsubscribed = listener.unsubscribed();
    assert_eq!(unsubscribed.len(), 1);
    assert_eq!(unsubscribed[0].0, "viewer-1");
    assert_eq!(unsubscribed[0].1, "/lidar");
    info!("on_unsubscribe callback validated: {:?}", unsubscribed[0]);

    viewer.close().await?;
    let _ = channel;
    gw.stop().await?;
    Ok(())
}

/// Test that disconnecting fires `on_unsubscribe` for all subscribed channels.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_disconnect_fires_unsubscribe_for_subscribed_channels() -> Result<()> {
    use std::sync::Arc;
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let channel = ctx
        .channel_builder("/imu")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
    let channel_id = advertise.channels[0].id;

    viewer.subscribe_and_wait(&[channel_id], &channel).await?;
    poll_until(|| listener.subscribed().len() == 1).await;

    // Disconnect — should fire on_unsubscribe for the subscribed channel.
    viewer.close().await?;
    poll_until(|| listener.unsubscribed().len() == 1).await;

    let unsubscribed = listener.unsubscribed();
    assert_eq!(unsubscribed.len(), 1);
    assert_eq!(unsubscribed[0].0, "viewer-1");
    assert_eq!(unsubscribed[0].1, "/imu");
    info!(
        "disconnect on_unsubscribe callback validated: {:?}",
        unsubscribed[0]
    );

    let _ = channel;
    gw.stop().await?;
    Ok(())
}

/// Test that closing a channel fires `on_unsubscribe` for active subscribers.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_channel_close_fires_unsubscribe_for_subscribers() -> Result<()> {
    use std::sync::Arc;
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let channel = ctx
        .channel_builder("/radar")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
    let channel_id = advertise.channels[0].id;

    viewer.subscribe_and_wait(&[channel_id], &channel).await?;
    poll_until(|| listener.subscribed().len() == 1).await;

    // Close the channel while the subscription is active.
    channel.close();

    // The viewer should receive an Unadvertise message.
    let unadvertise = viewer.expect_unadvertise().await?;
    assert_eq!(unadvertise.channel_ids, vec![channel_id]);

    // on_unsubscribe should have been called for the active subscription.
    poll_until(|| listener.unsubscribed().len() == 1).await;

    let unsubscribed = listener.unsubscribed();
    assert_eq!(unsubscribed.len(), 1);
    assert_eq!(unsubscribed[0].0, "viewer-1");
    assert_eq!(unsubscribed[0].1, "/radar");
    info!(
        "channel close on_unsubscribe callback validated: {:?}",
        unsubscribed[0]
    );

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

// ===========================================================================
// Client publish / message data tests
// ===========================================================================

/// Test that sending a client MessageData on the client-ch-{channelId} stream
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
            schema_name: String::new(),
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
    info!("on_message_data callback validated via per-channel stream");

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
            schema_name: String::new(),
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

/// Test that sending MessageData for a channel the client has not advertised produces an error.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_client_message_data_for_unadvertised_channel_sends_error() -> Result<()> {
    use std::sync::Arc;
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            capabilities: vec![foxglove::remote_access::Capability::ClientPublish],
            pending_client_reader_timeout: Some(Duration::from_secs(1)),
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    // Send MessageData for channel 999, which was never advertised by the client.
    // The server stashes the byte stream waiting for a matching Client Advertise.
    // With pending_client_reader_timeout set to 1s, the error arrives quickly.
    let payload = b"rogue data";
    viewer.send_client_message_data(999, payload).await?;

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
    assert!(
        status.message.contains("not advertised channel"),
        "unexpected status message: {}",
        status.message
    );
    info!("error status received: {}", status.message);

    // The listener should never have been called.
    assert!(
        listener.message_data().is_empty(),
        "listener should not receive message data for unadvertised channel"
    );

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
            schema_name: String::new(),
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

/// Test that connection status transitions occur in the correct order
/// (Connecting -> Connected -> ShuttingDown -> Shutdown) and that no
/// listener callbacks fire after Shutdown.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_connection_status_lifecycle() -> Result<()> {
    struct StatusTracker {
        statuses: Mutex<Vec<ConnectionStatus>>,
        callback_after_shutdown: Mutex<bool>,
    }

    impl foxglove::remote_access::Listener for StatusTracker {
        fn on_connection_status_changed(&self, status: ConnectionStatus) {
            let mut statuses = self.statuses.lock().unwrap();
            statuses.push(status);
        }

        fn on_subscribe(
            &self,
            _client: &foxglove::remote_access::Client,
            _channel: &foxglove::ChannelDescriptor,
        ) {
            let statuses = self.statuses.lock().unwrap();
            if statuses.last() == Some(&ConnectionStatus::Shutdown) {
                *self.callback_after_shutdown.lock().unwrap() = true;
            }
        }

        fn on_message_data(
            &self,
            _client: &foxglove::remote_access::Client,
            _channel: &foxglove::ChannelDescriptor,
            _payload: &[u8],
        ) {
            let statuses = self.statuses.lock().unwrap();
            if statuses.last() == Some(&ConnectionStatus::Shutdown) {
                *self.callback_after_shutdown.lock().unwrap() = true;
            }
        }
    }

    let tracker = Arc::new(StatusTracker {
        statuses: Mutex::new(Vec::new()),
        callback_after_shutdown: Mutex::new(false),
    });

    let ctx = foxglove::Context::new();

    // Create a channel so we can verify subscribe callbacks.
    let channel = ctx
        .channel_builder("/status-test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(tracker.clone()),
            ..Default::default()
        },
    )
    .await?;

    // Wait for Connected status.
    poll_until(|| {
        tracker
            .statuses
            .lock()
            .unwrap()
            .contains(&ConnectionStatus::Connected)
    })
    .await;
    assert_eq!(gw.handle.connection_status(), ConnectionStatus::Connected);

    // Connect a viewer and subscribe to trigger a listener callback while connected.
    let viewer = ViewerConnection::connect(&gw.room_name, "viewer-status").await?;
    viewer
        .subscribe_and_wait(&[u64::from(channel.id())], &channel)
        .await?;

    // Disconnect the viewer before stopping.
    viewer.close().await?;

    // Stop the gateway and wait for full shutdown.
    let runner = gw.handle.stop();
    tokio::time::timeout(remote_access_tests::test_helpers::SHUTDOWN_TIMEOUT, runner)
        .await
        .context("timeout waiting for gateway to stop")?
        .context("gateway runner panicked")?;

    // Validate status transitions.
    let statuses = tracker.statuses.lock().unwrap().clone();
    info!("recorded status transitions: {statuses:?}");

    assert_eq!(
        statuses,
        vec![
            ConnectionStatus::Connecting,
            ConnectionStatus::Connected,
            ConnectionStatus::ShuttingDown,
            ConnectionStatus::Shutdown,
        ],
    );

    // Verify no callbacks fired after Shutdown.
    assert!(
        !*tracker.callback_after_shutdown.lock().unwrap(),
        "listener callback fired after Shutdown"
    );

    Ok(())
}

// ===========================================================================
// Connection graph tests
// ===========================================================================

/// Test that subscribing to the connection graph delivers an empty initial update
/// when no graph has been published.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_connection_graph_subscribe_receives_empty_initial_state() -> Result<()> {
    let ctx = foxglove::Context::new();

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![foxglove::remote_access::Capability::ConnectionGraph],
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    viewer.send_subscribe_connection_graph().await?;
    let update = viewer.expect_connection_graph_update().await?;

    assert!(update.published_topics.is_empty());
    assert!(update.subscribed_topics.is_empty());
    assert!(update.advertised_services.is_empty());
    assert!(update.removed_topics.is_empty());
    assert!(update.removed_services.is_empty());
    info!("empty initial connection graph update validated");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that after subscribing to the connection graph and publishing a graph,
/// the viewer receives a ConnectionGraphUpdate with the published data.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_connection_graph_subscribe_and_publish() -> Result<()> {
    let ctx = foxglove::Context::new();

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![foxglove::remote_access::Capability::ConnectionGraph],
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    viewer.send_subscribe_connection_graph().await?;
    let _initial = viewer.expect_connection_graph_update().await?;

    let mut graph = ConnectionGraph::new();
    graph.set_published_topic("/camera", ["node_1"]);
    graph.set_subscribed_topic("/camera", ["node_2"]);
    graph.set_advertised_service("/set_mode", ["node_1"]);
    gw.handle.publish_connection_graph(graph)?;

    let update = viewer.expect_connection_graph_update().await?;
    assert_eq!(update.published_topics.len(), 1);
    assert_eq!(update.published_topics[0].name, "/camera");
    assert_eq!(update.published_topics[0].publisher_ids, vec!["node_1"]);

    assert_eq!(update.subscribed_topics.len(), 1);
    assert_eq!(update.subscribed_topics[0].name, "/camera");
    assert_eq!(update.subscribed_topics[0].subscriber_ids, vec!["node_2"]);

    assert_eq!(update.advertised_services.len(), 1);
    assert_eq!(update.advertised_services[0].name, "/set_mode");
    assert_eq!(update.advertised_services[0].provider_ids, vec!["node_1"]);
    info!("connection graph subscribe and publish validated");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that publishing a replacement connection graph delivers a diff update
/// containing only the changes (additions, modifications, and removals).
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_connection_graph_publish_diff_update() -> Result<()> {
    let ctx = foxglove::Context::new();

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![foxglove::remote_access::Capability::ConnectionGraph],
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    viewer.send_subscribe_connection_graph().await?;
    let _initial = viewer.expect_connection_graph_update().await?;

    // Publish a first graph.
    let mut graph1 = ConnectionGraph::new();
    graph1.set_published_topic("/camera", ["node_1"]);
    graph1.set_advertised_service("/set_mode", ["node_1"]);
    gw.handle.publish_connection_graph(graph1)?;
    let _update1 = viewer.expect_connection_graph_update().await?;

    // Publish a replacement that removes /camera, changes the service, and adds /lidar.
    let mut graph2 = ConnectionGraph::new();
    graph2.set_published_topic("/lidar", ["node_2"]);
    graph2.set_advertised_service("/set_mode", ["node_2"]);
    gw.handle.publish_connection_graph(graph2)?;

    let update2 = viewer.expect_connection_graph_update().await?;

    assert_eq!(update2.published_topics.len(), 1);
    assert_eq!(update2.published_topics[0].name, "/lidar");

    assert_eq!(update2.advertised_services.len(), 1);
    assert_eq!(update2.advertised_services[0].name, "/set_mode");
    assert_eq!(update2.advertised_services[0].provider_ids, vec!["node_2"]);

    assert_eq!(update2.removed_topics, vec!["/camera"]);
    assert!(
        update2.removed_services.is_empty(),
        "service was updated, not removed"
    );
    info!("connection graph diff update validated");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that unsubscribing from the connection graph prevents further updates
/// from being delivered.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_connection_graph_unsubscribe_stops_updates() -> Result<()> {
    let ctx = foxglove::Context::new();

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![foxglove::remote_access::Capability::ConnectionGraph],
            ..Default::default()
        },
    )
    .await?;

    let channel = ctx
        .channel_builder("/test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;
    let _advertise = viewer.expect_advertise().await?;

    // Subscribe to connection graph, then unsubscribe.
    viewer.send_subscribe_connection_graph().await?;
    let _initial = viewer.expect_connection_graph_update().await?;
    viewer.send_unsubscribe_connection_graph().await?;

    // Brief settle for the unsubscribe to be processed.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish a graph — the viewer should NOT receive this update.
    let mut graph = ConnectionGraph::new();
    graph.set_published_topic("/camera", ["node_1"]);
    gw.handle.publish_connection_graph(graph)?;

    // Subscribe to a channel and log a message to verify the data plane
    // is still working — we should receive MessageData but NOT a graph update.
    let cg_channel_id = u64::from(channel.id());
    viewer
        .subscribe_and_wait(&[cg_channel_id], &channel)
        .await?;
    viewer.ensure_device_data_track(cg_channel_id).await?;
    channel.log(b"ping");
    let msg = viewer
        .expect_new_data_track_and_message_data(cg_channel_id)
        .await?;
    assert_eq!(msg.data.as_ref(), b"ping");
    info!("connection graph unsubscribe validated: no graph update received after unsubscribe");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that subscribing to the connection graph without the ConnectionGraph
/// capability results in a Status error message.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_connection_graph_subscribe_without_capability_sends_error() -> Result<()> {
    let ctx = foxglove::Context::new();

    // Start gateway without ConnectionGraph capability.
    let gw = TestGateway::start(&ctx).await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    viewer.send_subscribe_connection_graph().await?;

    let status = viewer.expect_status().await?;
    assert_eq!(
        status.level,
        foxglove::protocol::v2::server::status::Level::Error
    );
    assert!(
        status.message.contains("connection graph"),
        "unexpected status message: {}",
        status.message
    );
    info!("error status received: {}", status.message);

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that `on_connection_graph_subscribe` fires when the first client subscribes
/// and `on_connection_graph_unsubscribe` fires when the last client unsubscribes.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_connection_graph_listener_callbacks() -> Result<()> {
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            capabilities: vec![foxglove::remote_access::Capability::ConnectionGraph],
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    // Subscribe — should fire on_connection_graph_subscribe (first subscriber).
    viewer.send_subscribe_connection_graph().await?;
    let _initial = viewer.expect_connection_graph_update().await?;
    poll_until(|| listener.connection_graph_subscribed_count() == 1).await;
    assert_eq!(listener.connection_graph_unsubscribed_count(), 0);
    info!("on_connection_graph_subscribe fired on first subscriber");

    // Unsubscribe — should fire on_connection_graph_unsubscribe (last subscriber).
    viewer.send_unsubscribe_connection_graph().await?;
    poll_until(|| listener.connection_graph_unsubscribed_count() == 1).await;
    assert_eq!(listener.connection_graph_subscribed_count(), 1);
    info!("on_connection_graph_unsubscribe fired on last unsubscribe");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that disconnecting a viewer that is subscribed to the connection graph
/// fires `on_connection_graph_unsubscribe` when it was the last subscriber.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_connection_graph_disconnect_cleans_up_subscription() -> Result<()> {
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            capabilities: vec![foxglove::remote_access::Capability::ConnectionGraph],
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    viewer.send_subscribe_connection_graph().await?;
    let _initial = viewer.expect_connection_graph_update().await?;
    poll_until(|| listener.connection_graph_subscribed_count() == 1).await;

    // Disconnect — should clean up subscription and fire unsubscribe callback.
    viewer.close().await?;
    poll_until(|| listener.connection_graph_unsubscribed_count() == 1).await;
    info!("disconnect cleaned up connection graph subscription");

    gw.stop().await?;
    Ok(())
}

/// Test first/last subscriber semantics with multiple viewers.
/// `on_connection_graph_subscribe` fires only on the first subscriber and
/// `on_connection_graph_unsubscribe` fires only when the last subscriber leaves.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_connection_graph_multiple_subscribers() -> Result<()> {
    let ctx = foxglove::Context::new();
    let listener = Arc::new(MockListener::default());

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            capabilities: vec![foxglove::remote_access::Capability::ConnectionGraph],
            ..Default::default()
        },
    )
    .await?;

    // First viewer subscribes — fires on_connection_graph_subscribe.
    let mut viewer1 = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _si1 = viewer1.expect_server_info().await?;
    viewer1.send_subscribe_connection_graph().await?;
    let _initial1 = viewer1.expect_connection_graph_update().await?;
    poll_until(|| listener.connection_graph_subscribed_count() == 1).await;

    // Second viewer subscribes — should NOT fire on_connection_graph_subscribe again.
    let mut viewer2 = ViewerConnection::connect(&gw.room_name, "viewer-2").await?;
    let _si2 = viewer2.expect_server_info().await?;
    viewer2.send_subscribe_connection_graph().await?;
    let _initial2 = viewer2.expect_connection_graph_update().await?;

    // Brief settle to ensure no extra callback fires.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(
        listener.connection_graph_subscribed_count(),
        1,
        "on_connection_graph_subscribe should only fire once for the first subscriber"
    );

    // Publish a graph — both viewers should receive the update.
    let mut graph = ConnectionGraph::new();
    graph.set_published_topic("/camera", ["node_1"]);
    gw.handle.publish_connection_graph(graph)?;

    let update1 = viewer1.expect_connection_graph_update().await?;
    let update2 = viewer2.expect_connection_graph_update().await?;
    assert_eq!(update1.published_topics.len(), 1);
    assert_eq!(update2.published_topics.len(), 1);
    info!("both subscribers received graph update");

    // Disconnect first viewer — should NOT fire on_connection_graph_unsubscribe.
    viewer1.close().await?;
    // Wait for the gateway to process the disconnect.
    viewer2
        .wait_for_participant_disconnected("viewer-1")
        .await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        listener.connection_graph_unsubscribed_count(),
        0,
        "on_connection_graph_unsubscribe should not fire while subscribers remain"
    );

    // Disconnect second (last) viewer — should fire on_connection_graph_unsubscribe.
    viewer2.close().await?;
    poll_until(|| listener.connection_graph_unsubscribed_count() == 1).await;
    info!("on_connection_graph_unsubscribe fired when last subscriber disconnected");

    gw.stop().await?;
    Ok(())
}

/// Test that updating the connection graph while no session is active (e.g. before
/// the gateway has connected) persists the state, and a viewer subscribing after
/// the session is established receives the latest graph.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_connection_graph_persists_when_no_session() -> Result<()> {
    let ctx = foxglove::Context::new();

    let (room_name, mock) = TestGateway::prepare().await;

    let gw = TestGateway::start_with_mock(
        &ctx,
        room_name,
        mock,
        TestGatewayOptions {
            capabilities: vec![foxglove::remote_access::Capability::ConnectionGraph],
            ..Default::default()
        },
    )?;

    // The gateway was just started and is still connecting (no session yet).
    // Publish a connection graph in this state — it should be stored on the
    // connection and delivered to subscribers once a session is established.
    assert_eq!(
        gw.handle.connection_status(),
        ConnectionStatus::Connecting,
        "gateway should still be connecting"
    );

    let mut graph = ConnectionGraph::new();
    graph.set_published_topic("/persisted_topic", ["node_1"]);
    graph.set_subscribed_topic("/persisted_topic", ["node_2"]);
    graph.set_advertised_service("/persisted_service", ["node_1"]);
    gw.handle.publish_connection_graph(graph)?;

    // Now wait for the gateway to connect.
    poll_until(|| gw.handle.connection_status() == ConnectionStatus::Connected).await;

    // Connect a viewer and subscribe to the connection graph.
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    viewer.send_subscribe_connection_graph().await?;
    let update = viewer.expect_connection_graph_update().await?;

    // Verify the initial update contains the graph state that was published
    // before the session was established.
    assert_eq!(update.published_topics.len(), 1);
    assert_eq!(update.published_topics[0].name, "/persisted_topic");
    assert_eq!(update.published_topics[0].publisher_ids, vec!["node_1"]);

    assert_eq!(update.subscribed_topics.len(), 1);
    assert_eq!(update.subscribed_topics[0].name, "/persisted_topic");
    assert_eq!(update.subscribed_topics[0].subscriber_ids, vec!["node_2"]);

    assert_eq!(update.advertised_services.len(), 1);
    assert_eq!(update.advertised_services[0].name, "/persisted_service");
    assert_eq!(update.advertised_services[0].provider_ids, vec!["node_1"]);
    info!("connection graph persisted across no-session gap validated");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

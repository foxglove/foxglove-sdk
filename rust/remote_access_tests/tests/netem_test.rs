//! Integration tests that validate WebRTC behavior under simulated network
//! impairment (latency, jitter, packet loss) using a netem sidecar container.
//!
//! These tests require the netem Docker Compose overlay:
//!   docker compose -f docker-compose.yaml -f docker-compose.netem.yml up -d --wait
//!
//! Run with: `cargo test -p remote_access_tests -- --ignored netem_`
//!
//! The netem sidecar applies tc/netem rules to the LiveKit container's network
//! namespace, shaping all egress traffic (including RTC media/data). Configure
//! impairment via the `NETEM_ARGS` environment variable (see
//! `docker-compose.netem.yml` for details).

use anyhow::{Context as _, Result};
use remote_access_tests::test_helpers::{TestGateway, ViewerConnection};
use tracing::info;
use tracing_test::traced_test;

// ===========================================================================
// Tests
// ===========================================================================

/// Verify that a viewer can connect and receive a valid ServerInfo message
/// under network impairment. This is the basic "connectivity still works" check.
#[traced_test]
#[ignore]
#[tokio::test]
async fn netem_viewer_connects_under_impairment() -> Result<()> {
    let ctx = foxglove::Context::new();
    let gw = TestGateway::start(&ctx).await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let server_info = viewer.expect_server_info().await?;

    assert!(
        server_info.session_id.is_some(),
        "session_id should be present"
    );
    info!("ServerInfo received under impairment: {server_info:?}");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Verify that channel advertisements are delivered under impairment.
#[traced_test]
#[ignore]
#[tokio::test]
async fn netem_channel_advertisement_under_impairment() -> Result<()> {
    let ctx = foxglove::Context::new();

    let channel = ctx
        .channel_builder("/netem-test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;

    assert_eq!(advertise.channels.len(), 1);
    assert_eq!(advertise.channels[0].topic, "/netem-test");
    assert_eq!(advertise.channels[0].id, u64::from(channel.id()));
    info!("channel advertisement received under impairment");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Verify that the full subscribe-and-receive flow works under impairment.
/// A single message is logged after subscribing and the viewer must receive it.
#[traced_test]
#[ignore]
#[tokio::test]
async fn netem_message_delivery_under_impairment() -> Result<()> {
    let ctx = foxglove::Context::new();
    let channel = ctx
        .channel_builder("/netem-test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
    let channel_id = advertise.channels[0].id;

    viewer.subscribe_and_wait(&[channel_id], &channel).await?;

    let payload = b"netem-hello";
    channel.log(payload);

    let msg = viewer.expect_message_data().await?;
    assert_eq!(msg.channel_id, channel_id);
    assert_eq!(msg.data.as_ref(), payload);
    info!("message delivered under impairment");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Verify that a burst of messages is delivered completely and in order under
/// impairment. Netem jitter can reorder packets at the IP level, but LiveKit's
/// reliable byte stream should compensate.
#[traced_test]
#[ignore]
#[tokio::test]
async fn netem_burst_delivery_under_impairment() -> Result<()> {
    let ctx = foxglove::Context::new();
    let channel = ctx
        .channel_builder("/netem-burst")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
    let channel_id = advertise.channels[0].id;

    viewer.subscribe_and_wait(&[channel_id], &channel).await?;

    // Send a burst of messages.
    let count = 20;
    for i in 0..count {
        let payload = format!("msg-{i:04}");
        channel.log(payload.as_bytes());
    }

    // Verify all messages arrive in order.
    for i in 0..count {
        let msg = viewer.expect_message_data().await?;
        let expected = format!("msg-{i:04}");
        assert_eq!(msg.channel_id, channel_id);
        assert_eq!(
            msg.data.as_ref(),
            expected.as_bytes(),
            "message {i} out of order or missing"
        );
    }
    info!("all {count} messages delivered in order under impairment");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

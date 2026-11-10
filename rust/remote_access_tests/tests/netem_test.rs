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

use std::time::Duration;

use anyhow::{Context as _, Result};
use foxglove::remote_access::{Capability, ConnectionGraph};
use remote_access_tests::test_helpers::{
    NETEM_EVENT_TIMEOUT, TestGateway, TestGatewayOptions, ViewerConnection,
};
use serial_test::serial;
use tracing::info;
use tracing_test::traced_test;

// ===========================================================================
// WebRTC under impairment
// ===========================================================================

/// Verify that a viewer can connect and receive a valid ServerInfo message
/// under network impairment. This is the basic "connectivity still works" check.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(netem)]
async fn netem_viewer_connects_under_impairment() -> Result<()> {
    let ctx = foxglove::Context::new();
    let gw = TestGateway::start(&ctx).await?;

    // This test has no channels, so the gateway won't send an Advertise.
    let expect_advertise = false;
    let (viewer, server_info, _advertise) = ViewerConnection::connect_and_await_startup(
        &gw.room_name,
        "viewer-1",
        expect_advertise,
        NETEM_EVENT_TIMEOUT,
    )
    .await?;
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
#[serial(netem)]
async fn netem_channel_advertisement_under_impairment() -> Result<()> {
    let ctx = foxglove::Context::new();

    let channel = ctx
        .channel_builder("/netem-test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let expect_advertise = true;
    let (viewer, _server_info, advertise) = ViewerConnection::connect_and_await_startup(
        &gw.room_name,
        "viewer-1",
        expect_advertise,
        NETEM_EVENT_TIMEOUT,
    )
    .await?;

    assert_eq!(advertise.channels.len(), 1);
    assert_eq!(advertise.channels[0].topic, "/netem-test");
    assert_eq!(advertise.channels[0].id, u64::from(channel.id()));
    info!("channel advertisement received under impairment");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Verify that the full subscribe-and-receive flow works under impairment.
/// Data tracks are lossy, so the message is sent repeatedly every 1ms until
/// the viewer receives it (or the test times out).
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(netem)]
async fn netem_message_delivery_under_impairment() -> Result<()> {
    let ctx = foxglove::Context::new();
    let channel = ctx
        .channel_builder("/netem-test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let expect_advertise = true;
    let (mut viewer, _server_info, advertise) = ViewerConnection::connect_and_await_startup(
        &gw.room_name,
        "viewer-1",
        expect_advertise,
        NETEM_EVENT_TIMEOUT,
    )
    .await?;
    let channel_id = advertise.channels[0].id;

    viewer.subscribe_and_wait(&[channel_id], &channel).await?;

    let payload = b"netem-hello";
    let sender_channel = channel.clone();
    let sender = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(1));
        loop {
            interval.tick().await;
            sender_channel.log(payload);
        }
    });

    let msg = viewer
        .expect_new_data_track_and_message_data(channel_id)
        .await?;
    sender.abort();
    assert_eq!(msg.data.as_ref(), payload);
    info!("message delivered under impairment");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Verify that a burst of control-plane messages is delivered completely and in
/// order under impairment. Connection graph updates travel over the control
/// channel (not the lossy data tracks), so the reliable byte stream should
/// guarantee ordered delivery despite netem jitter and packet loss.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(netem)]
async fn netem_burst_delivery_under_impairment() -> Result<()> {
    let ctx = foxglove::Context::new();

    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::ConnectionGraph],
            ..Default::default()
        },
    )
    .await?;

    // This test has no channels, so the gateway won't send an Advertise.
    let expect_advertise = false;
    let (mut viewer, _server_info, _advertise) = ViewerConnection::connect_and_await_startup(
        &gw.room_name,
        "viewer-1",
        expect_advertise,
        NETEM_EVENT_TIMEOUT,
    )
    .await?;

    viewer.send_subscribe_connection_graph().await?;
    let _initial = viewer.expect_connection_graph_update().await?;

    let count = 20;
    for i in 0..count {
        let mut graph = ConnectionGraph::new();
        graph.set_published_topic(format!("/burst-{i:02}"), [format!("node-{i}")]);
        gw.handle.publish_connection_graph(graph)?;
    }

    for i in 0..count {
        let update = viewer.expect_connection_graph_update().await?;
        let expected_topic = format!("/burst-{i:02}");

        assert_eq!(
            update.published_topics.len(),
            1,
            "update {i}: expected exactly one published topic, got {:?}",
            update.published_topics,
        );
        assert_eq!(
            update.published_topics[0].name, expected_topic,
            "update {i} out of order or missing"
        );

        if i > 0 {
            let prev_topic = format!("/burst-{:02}", i - 1);
            assert!(
                update.removed_topics.contains(&prev_topic),
                "update {i}: expected removed topic {prev_topic}, got {:?}",
                update.removed_topics,
            );
        }
    }
    info!("all {count} connection graph updates delivered in order under impairment");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

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

mod netem_helpers;

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
// Sidecar validation
// ===========================================================================

/// Verify that the netem sidecar is actually delaying traffic. Without netem,
/// the LiveKit health endpoint (port 7880) responds in under 5ms. With netem
/// delay configured, each egress packet is delayed, so TCP round-trips take
/// noticeably longer.
///
/// The threshold is derived from the `NETEM_ARGS` environment variable (the
/// same one that drives the sidecar). If `NETEM_ARGS` contains no `delay`
/// keyword, the latency assertion is skipped.
///
/// This is the foundational smoke test: if this fails, the sidecar isn't
/// working and the other netem tests are meaningless.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(netem)]
async fn netem_sidecar_adds_measurable_latency() -> Result<()> {
    // Read the same env var the compose sidecar uses, falling back to the
    // default defined in docker-compose.netem.yml.
    let netem_args = netem_helpers::default_netem_args();
    info!("NETEM_ARGS: {netem_args}");

    // Parse the delay value (in ms) from NETEM_ARGS. Format is "delay <N>ms ...".
    let configured_delay_ms: Option<u64> = netem_helpers::parse_delay_ms(&netem_args);

    if configured_delay_ms.is_none() {
        info!("no delay configured in NETEM_ARGS — skipping latency assertion");
        return Ok(());
    }
    let delay_ms = configured_delay_ms.unwrap();

    let client = reqwest::Client::new();

    // Make several requests and collect response times.
    let mut durations = Vec::new();
    for i in 0..5 {
        let start = tokio::time::Instant::now();
        let status = client.get("http://localhost:7880").send().await?.status();
        let elapsed = start.elapsed();
        assert!(status.is_success(), "health check failed: {status}");
        info!("request {i}: {elapsed:?}");
        durations.push(elapsed);
    }

    // Sort and take the median to filter out outliers.
    durations.sort();
    let median = durations[durations.len() / 2];

    // Use 1/3 of the configured delay as a conservative threshold. Without
    // netem this endpoint responds in <1ms, so any real delay is detectable.
    let threshold = Duration::from_millis(delay_ms / 3);
    assert!(
        median > threshold,
        "netem does not appear active: median response time was {median:?}, \
         expected >{threshold:?} (configured delay: {delay_ms}ms)"
    );
    info!("median response time: {median:?} (threshold: {threshold:?}) — netem is working");
    Ok(())
}

/// Verify that the netem sidecar is actually dropping packets. Sends a burst of
/// UDP datagrams to a socat echo server running inside the container and counts
/// how many echo responses come back. Responses traverse the netem-shaped egress
/// path, so a fraction will be lost.
///
/// The configured loss percentage is read from `NETEM_ARGS`. If no `loss` keyword
/// is present, the assertion is skipped.
///
/// With 500 packets at 2% loss, the probability of a false failure (zero
/// drops despite loss being configured) is roughly 4 in 100,000.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(netem)]
async fn netem_sidecar_drops_packets() -> Result<()> {
    let netem_args = netem_helpers::default_netem_args();
    info!("NETEM_ARGS: {netem_args}");

    // Parse loss percentage from NETEM_ARGS. Format: "... loss <N>% ...".
    let loss_pct: Option<f64> = netem_helpers::parse_loss_percentage(&netem_args);

    if loss_pct.is_none() || loss_pct < Some(2.0) {
        info!("loss < 2% configured in NETEM_ARGS — skipping (need ≥2% for reliable detection)");
        return Ok(());
    }
    let loss_pct = loss_pct.unwrap();

    let sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
    let dest: std::net::SocketAddr = "127.0.0.1:9999".parse().unwrap();

    // Send a burst of UDP packets to the echo server inside the container.
    let sent: u32 = 500;
    for i in 0..sent {
        let msg = format!("pkt-{i:04}");
        sock.send_to(msg.as_bytes(), dest).await?;
    }

    // Collect echo responses. Use a generous timeout to accommodate any
    // configured netem delay.
    let mut received: u32 = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    let mut buf = [0u8; 64];
    loop {
        match tokio::time::timeout_at(deadline, sock.recv_from(&mut buf)).await {
            Ok(Ok(_)) => received += 1,
            _ => break,
        }
        if received == sent {
            break;
        }
    }

    let lost = sent - received;
    let observed_loss = (lost as f64 / sent as f64) * 100.0;
    info!("sent: {sent}, received: {received}, lost: {lost} ({observed_loss:.1}%)");
    info!("configured loss: {loss_pct}%");

    // Verify the echo server is reachable — at least some packets must arrive.
    assert!(
        received > 0,
        "no echo responses received — is the netem stack running? \
         Start with: docker compose -f docker-compose.yaml \
         -f docker-compose.netem.yml up -d --wait"
    );

    // Verify that netem is actually dropping some packets.
    assert!(
        lost > 0,
        "expected some packet loss with {loss_pct}% configured, \
         but all {sent} packets were echoed back"
    );
    Ok(())
}

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

    let mut viewer =
        ViewerConnection::connect_with_timeout(&gw.room_name, "viewer-1", NETEM_EVENT_TIMEOUT)
            .await?;
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
#[serial(netem)]
async fn netem_channel_advertisement_under_impairment() -> Result<()> {
    let ctx = foxglove::Context::new();

    let channel = ctx
        .channel_builder("/netem-test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer =
        ViewerConnection::connect_with_timeout(&gw.room_name, "viewer-1", NETEM_EVENT_TIMEOUT)
            .await?;

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
    let mut viewer =
        ViewerConnection::connect_with_timeout(&gw.room_name, "viewer-1", NETEM_EVENT_TIMEOUT)
            .await?;

    let _server_info = viewer.expect_server_info().await?;
    let advertise = viewer.expect_advertise().await?;
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

    let mut viewer =
        ViewerConnection::connect_with_timeout(&gw.room_name, "viewer-1", NETEM_EVENT_TIMEOUT)
            .await?;

    let _server_info = viewer.expect_server_info().await?;

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

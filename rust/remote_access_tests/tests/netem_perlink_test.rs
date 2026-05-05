//! Integration tests for per-link network impairment using classful qdiscs
//! (HTB root + netem leaf classes).
//!
//! **Infrastructure tests** validate the tc hierarchy (qdisc, class, filter
//! setup) and per-link impairment differentiation (latency, loss).
//!
//! **Product tests** verify that the SDK works correctly under the classful
//! qdisc hierarchy, which is structurally different from the flat `root netem`
//! used in `netem_test.rs`. Host traffic (via Docker port forwarding) hits the
//! HTB default class, exercising the classful path.
//!
//! These tests require the per-link Docker Compose overlay:
//!   docker compose \
//!     -f docker-compose.yaml \
//!     -f docker-compose.netem.yml \
//!     -f docker-compose.netem-perlink.yml \
//!     up -d --wait
//!
//! Run with: `cargo test -p remote_access_tests -- --ignored perlink_`
//!
//! The per-link overlay creates two target containers on a custom network
//! (10.99.0.0/24) with static IPs. The netem sidecar classifies egress traffic
//! by destination IP, applying different impairment profiles to each link:
//!   - Link A (10.99.0.10): high impairment (default: 200ms delay, 5% loss).
//!   - Link B (10.99.0.20): low impairment (default: 10ms delay, no loss).
//!
//! Tests run network probes from within the netem sidecar (via `docker exec`)
//! because the netem qdisc only shapes egress from that network namespace.

mod netem_helpers;

use std::process::Command;
use std::time::Duration;

use anyhow::{Context as _, Result};
use remote_access_tests::test_helpers::{NETEM_EVENT_TIMEOUT, TestGateway, ViewerConnection};
use serial_test::serial;
use tracing::info;
use tracing_test::traced_test;

/// IP address of target-a (high impairment link).
const TARGET_A_IP: &str = "10.99.0.10";
/// IP address of target-b (low impairment link).
const TARGET_B_IP: &str = "10.99.0.20";
/// TCP echo port on targets.
const TARGET_TCP_PORT: u16 = 7000;
/// UDP echo port on targets.
const TARGET_UDP_PORT: u16 = 7001;

/// Default netem args for each link, matching docker-compose.netem-perlink.yml.
const DEFAULT_LINK_A_ARGS: &str = "delay 200ms 50ms loss 5%";
const DEFAULT_LINK_B_ARGS: &str = "delay 10ms 2ms";

/// Execute a command inside the netem container and return stdout.
fn docker_exec(container: &str, cmd: &[&str]) -> Result<String> {
    let output = Command::new("docker")
        .arg("exec")
        .arg(container)
        .args(cmd)
        .output()
        .context("failed to run docker exec")?;

    anyhow::ensure!(
        output.status.success(),
        "docker exec failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).context("invalid UTF-8 from docker exec")
}

/// Measure TCP round-trip time to a target by timing a small echo exchange
/// from within the netem container. Returns the median of `count` measurements.
///
/// Note: the measured RTT includes `docker exec` overhead (~50-200ms), not just
/// netem delay. This is acceptable because both links include the same overhead,
/// so relative comparisons (A > B) remain valid. Absolute thresholds in
/// assertions are intentionally conservative to account for this.
fn measure_tcp_rtt(container: &str, target_ip: &str, count: usize) -> Result<Duration> {
    let mut durations = Vec::new();

    for _ in 0..count {
        let start = std::time::Instant::now();
        // Send "ping\n" to the TCP echo server and read the echo back.
        // Timeout after 5 seconds to avoid hanging on lost connections.
        let result = docker_exec(
            container,
            &[
                "sh",
                "-c",
                &format!("echo ping | socat -T5 - TCP:{target_ip}:{TARGET_TCP_PORT}"),
            ],
        );

        match result {
            Ok(_) => {
                let elapsed = start.elapsed();
                durations.push(elapsed);
            }
            Err(e) => {
                info!("TCP echo to {target_ip} failed (may be expected under impairment): {e}");
            }
        }
    }

    anyhow::ensure!(
        !durations.is_empty(),
        "all TCP echo attempts to {target_ip} failed"
    );

    durations.sort();
    Ok(durations[durations.len() / 2])
}

/// Send UDP datagrams from within the netem container to a target echo server
/// and count how many echo responses come back. Returns (sent, received).
///
/// Each packet is sent individually via `socat UDP-CONNECT` with a short
/// timeout. The echo server reflects the datagram back through the netem-shaped
/// egress path. Packets that are dropped by netem will time out.
fn measure_udp_loss(container: &str, target_ip: &str, count: u32) -> Result<(u32, u32)> {
    // Send each packet individually and count successful echoes. Using
    // `UDP-CONNECT` creates a connected socket so socat waits for the response.
    // Two timeouts work together: `socat -T1` aborts after 1s of inactivity
    // (no data received), while `timeout 2` is a hard wall-clock cap. The -T1
    // is the binding constraint for lost packets; the outer timeout is a safety
    // net in case socat hangs for other reasons.
    let script = format!(
        r#"
        received=0
        for i in $(seq 1 {count}); do
            resp=$(echo "pkt-$i" | timeout 2 socat -T1 - UDP-CONNECT:{target_ip}:{TARGET_UDP_PORT} 2>/dev/null)
            if [ -n "$resp" ]; then
                received=$((received + 1))
            fi
        done
        echo "{count} $received"
        "#
    );

    let output = docker_exec(container, &["sh", "-c", &script])?;
    let parts: Vec<&str> = output
        .trim()
        .lines()
        .last()
        .unwrap_or("")
        .split_whitespace()
        .collect();

    anyhow::ensure!(
        parts.len() == 2,
        "unexpected output from UDP loss measurement: {output:?}"
    );

    let sent: u32 = parts[0].parse().context("failed to parse sent count")?;
    let received: u32 = parts[1].parse().context("failed to parse received count")?;
    Ok((sent, received))
}

// ===========================================================================
// Tests
// ===========================================================================

/// Verify that the per-link qdisc hierarchy is correctly installed. Checks that
/// `tc qdisc show` output contains HTB and netem entries for both links.
#[traced_test]
#[ignore]
#[test]
fn perlink_infra_qdisc_hierarchy_is_installed() -> Result<()> {
    let container = netem_helpers::netem_container_id()?;

    let qdisc_output = docker_exec(&container, &["tc", "qdisc", "show"])?;
    info!("tc qdisc show:\n{qdisc_output}");

    assert!(
        qdisc_output.contains("htb"),
        "expected HTB root qdisc, got:\n{qdisc_output}"
    );
    assert!(
        qdisc_output.contains("netem"),
        "expected netem leaf qdiscs, got:\n{qdisc_output}"
    );

    // Check filters on all interfaces — the perlink network may not be on eth0.
    let filter_output = docker_exec(
        &container,
        &[
            "sh",
            "-c",
            "for iface in $(ls /sys/class/net/); do tc filter show dev $iface 2>/dev/null; done",
        ],
    )?;
    info!("tc filter show (all interfaces):\n{filter_output}");

    assert!(
        filter_output.contains("u32"),
        "expected u32 filters on at least one interface, got:\n{filter_output}"
    );

    Ok(())
}

/// Verify that link A (high impairment) has measurably higher latency than
/// link B (low impairment). Measures TCP round-trip times from within the netem
/// container to each target.
#[traced_test]
#[ignore]
#[test]
fn perlink_infra_link_a_has_higher_latency_than_link_b() -> Result<()> {
    let link_a_args =
        std::env::var("NETEM_LINK_A_ARGS").unwrap_or_else(|_| DEFAULT_LINK_A_ARGS.into());
    let link_b_args =
        std::env::var("NETEM_LINK_B_ARGS").unwrap_or_else(|_| DEFAULT_LINK_B_ARGS.into());

    let delay_a = netem_helpers::parse_delay_ms(&link_a_args);
    let delay_b = netem_helpers::parse_delay_ms(&link_b_args);

    let (Some(delay_a_ms), Some(delay_b_ms)) = (delay_a, delay_b) else {
        info!("cannot compare latency — one or both links have no delay configured");
        return Ok(());
    };

    anyhow::ensure!(
        delay_a_ms > delay_b_ms,
        "test expects link A delay ({delay_a_ms}ms) > link B delay ({delay_b_ms}ms)"
    );

    let container = netem_helpers::netem_container_id()?;
    let measurement_count = 5;

    let rtt_a = measure_tcp_rtt(&container, TARGET_A_IP, measurement_count)?;
    let rtt_b = measure_tcp_rtt(&container, TARGET_B_IP, measurement_count)?;

    info!("link A median RTT: {rtt_a:?} (configured delay: {delay_a_ms}ms)");
    info!("link B median RTT: {rtt_b:?} (configured delay: {delay_b_ms}ms)");

    assert!(
        rtt_a > rtt_b,
        "expected link A RTT ({rtt_a:?}) > link B RTT ({rtt_b:?}), \
         but link A should have higher impairment"
    );

    // Verify link A RTT is at least partially explained by the configured delay.
    let threshold_a = Duration::from_millis(delay_a_ms / 3);
    assert!(
        rtt_a > threshold_a,
        "link A RTT ({rtt_a:?}) is too low for configured delay ({delay_a_ms}ms), \
         expected > {threshold_a:?}"
    );

    Ok(())
}

/// Verify that link A (with loss configured) drops more packets than link B
/// (no loss configured). Sends UDP bursts from within the netem container.
#[traced_test]
#[ignore]
#[test]
fn perlink_infra_link_a_has_more_packet_loss_than_link_b() -> Result<()> {
    let link_a_args =
        std::env::var("NETEM_LINK_A_ARGS").unwrap_or_else(|_| DEFAULT_LINK_A_ARGS.into());
    let link_b_args =
        std::env::var("NETEM_LINK_B_ARGS").unwrap_or_else(|_| DEFAULT_LINK_B_ARGS.into());

    let loss_a_pct = netem_helpers::parse_loss_percentage(&link_a_args).unwrap_or(0.0);
    let loss_b_pct = netem_helpers::parse_loss_percentage(&link_b_args).unwrap_or(0.0);

    if loss_a_pct < 2.0 {
        info!("link A loss ({loss_a_pct}%) too low for reliable detection — skipping");
        return Ok(());
    }

    let container = netem_helpers::netem_container_id()?;
    // 150 packets at 5% one-way loss (egress only; echoes return unshaped)
    // gives P(0 lost) ≈ 0.95^150 ≈ 5e-4, making false failures negligible.
    let packet_count: u32 = 150;

    let (sent_a, received_a) = measure_udp_loss(&container, TARGET_A_IP, packet_count)?;
    let (sent_b, received_b) = measure_udp_loss(&container, TARGET_B_IP, packet_count)?;

    assert!(
        received_a <= sent_a && received_b <= sent_b,
        "received more packets than sent (A: {received_a}/{sent_a}, B: {received_b}/{sent_b})"
    );
    let lost_a = sent_a.saturating_sub(received_a);
    let lost_b = sent_b.saturating_sub(received_b);
    let loss_rate_a = (lost_a as f64 / sent_a as f64) * 100.0;
    let loss_rate_b = (lost_b as f64 / sent_b as f64) * 100.0;

    info!("link A: sent={sent_a}, received={received_a}, lost={lost_a} ({loss_rate_a:.1}%)");
    info!("link B: sent={sent_b}, received={received_b}, lost={lost_b} ({loss_rate_b:.1}%)");
    info!("configured: link A loss={loss_a_pct}%, link B loss={loss_b_pct}%");

    // Link A should have some packet loss.
    assert!(
        lost_a > 0,
        "expected some packet loss on link A with {loss_a_pct}% configured, \
         but all {sent_a} packets were echoed back"
    );

    // If link B has no loss configured, it should have perfect or near-perfect delivery.
    if loss_b_pct < 1.0 {
        assert!(
            lost_a > lost_b,
            "expected link A to lose more packets than link B: \
             A lost {lost_a}, B lost {lost_b}"
        );
    }

    Ok(())
}

// ===========================================================================
// Product tests under classful qdisc hierarchy
// ===========================================================================
//
// These tests exercise the SDK through the HTB default class. Host traffic
// (via Docker port forwarding) is classified into the default HTB class,
// which has its own netem leaf qdisc (matching NETEM_ARGS). This validates
// that SDK connectivity and message delivery work under the classful
// hierarchy, not just the flat `root netem` used in `netem_test.rs`.

/// Verify that a viewer can connect and receive a valid ServerInfo message
/// under the classful qdisc hierarchy.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(netem)]
async fn perlink_product_viewer_connects_under_classful_qdisc() -> Result<()> {
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
    info!("ServerInfo received under classful qdisc: {server_info:?}");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Verify that data track message delivery works under the classful qdisc
/// hierarchy. Data tracks are lossy, so the message is sent repeatedly until the
/// viewer receives it (or the test times out). This validates that the data plane
/// functions correctly under classful qdiscs (HTB root + netem leaf), which is
/// structurally different from the flat `root netem` tested in `netem_test.rs`.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(netem)]
async fn perlink_product_data_track_delivery_under_classful_qdisc() -> Result<()> {
    let ctx = foxglove::Context::new();
    let channel = ctx
        .channel_builder("/perlink-data-track")
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

    let payload = b"perlink-hello";
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
    info!("data track message delivered under classful qdisc");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Verify that traffic to an IP matching neither link A nor link B falls into
/// the default class. The default class uses NETEM_ARGS, which has its own
/// delay. We verify by checking that the tc hierarchy has a default class with
/// netem attached.
#[traced_test]
#[ignore]
#[test]
fn perlink_infra_default_class_catches_unclassified_traffic() -> Result<()> {
    let container = netem_helpers::netem_container_id()?;

    // Verify the qdisc hierarchy has a default class. Check all interfaces since
    // the perlink network may not be on eth0.
    let class_output = docker_exec(
        &container,
        &[
            "sh",
            "-c",
            "for iface in $(ls /sys/class/net/); do tc class show dev $iface 2>/dev/null; done",
        ],
    )?;
    info!("tc class show (all interfaces):\n{class_output}");

    // The default class should exist (classid ff00 in our setup).
    assert!(
        class_output.contains("ff00"),
        "expected default class ff00 in tc class output:\n{class_output}"
    );

    // Verify the default class has a netem qdisc attached.
    let qdisc_output = docker_exec(&container, &["tc", "qdisc", "show"])?;
    assert!(
        qdisc_output.contains("ff00"),
        "expected netem qdisc on default class ff00:\n{qdisc_output}"
    );

    info!("default class ff00 verified with netem qdisc attached");
    Ok(())
}

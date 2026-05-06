//! Integration test for control stream write-failure recovery under network
//! partition (FLE-384).
//!
//! Verifies the full recovery flow added in PR #1102 (FLE-364): when a network
//! partition causes control stream writes to fail, the gateway's
//! `reset_participant` path tears down and reinitializes the participant. After
//! the partition is lifted, a reconnecting viewer receives a fresh `ServerInfo`
//! and advertisements for all channels — including those created during the
//! partition.
//!
//! These tests require the netem Docker Compose overlay:
//!   docker compose -f docker-compose.yaml -f docker-compose.netem.yml up -d --wait
//!
//! Run with: `cargo test -p remote_access_tests -- --ignored netem_partition_`

mod netem_helpers;

use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context as _, Result};
use remote_access_tests::test_helpers::{NETEM_EVENT_TIMEOUT, TestGateway, ViewerConnection};
use serial_test::serial;
use tracing::info;

/// Set up a tracing subscriber that captures logs from both the test crate and
/// the `foxglove` crate. The default `#[traced_test]` filter only captures
/// `remote_access_tests=trace`, which misses the `reset_participant` warn from
/// the `foxglove` crate. We need to see that log to verify recovery.
fn init_tracing() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let mock_writer = tracing_test::internal::MockWriter::new(global_log_buf());
        let subscriber = tracing_test::internal::get_subscriber(
            mock_writer,
            "foxglove::remote_access=warn,remote_access_tests=trace",
        );
        tracing::dispatcher::set_global_default(subscriber)
            .expect("could not set global tracing subscriber");
    });
}

fn global_log_buf() -> &'static Mutex<Vec<u8>> {
    static BUF: std::sync::OnceLock<Mutex<Vec<u8>>> = std::sync::OnceLock::new();
    BUF.get_or_init(|| Mutex::new(Vec::new()))
}

fn logs_contain(needle: &str) -> bool {
    let buf = global_log_buf().lock().unwrap();
    let logs = String::from_utf8_lossy(&buf);
    logs.contains(needle)
}

/// Verify that a network partition triggers the `reset_participant` recovery
/// path and that the reconnecting viewer sees a consistent channel registry.
///
/// 1. Connect viewer-1 and verify initial `ServerInfo` + channel advertisement.
/// 2. Impose a full partition (100% packet loss).
/// 3. Create a second channel on the gateway while the partition blocks egress.
/// 4. Wait for the gateway's flush-task to hit a write failure, triggering
///    `reset_participant`.
/// 5. Lift the partition, restoring connectivity.
/// 6. Reconnect as the same viewer-1 identity.
/// 7. Assert that `reset_participant` fired (via its tracing warn).
/// 8. Verify viewer-1 receives a fresh `ServerInfo` and advertisements for
///    *both* channels — including the one created during the partition.
#[ignore]
#[tokio::test]
#[serial(netem)]
async fn netem_partition_recovery_readvertises_all_channels() -> Result<()> {
    init_tracing();

    let container = netem_helpers::netem_container_id()?;
    let ctx = foxglove::Context::new();

    // Create the first channel before any viewer connects.
    let channel_a = ctx
        .channel_builder("/partition-test/before")
        .message_encoding("json")
        .build_raw()
        .context("create channel A")?;

    let gw = TestGateway::start(&ctx).await?;

    // Phase 1: Verify initial connectivity.
    let mut viewer =
        ViewerConnection::connect_with_timeout(&gw.room_name, "viewer-1", NETEM_EVENT_TIMEOUT)
            .await?;
    let server_info_1 = viewer.expect_server_info().await?;
    let advertise_1 = viewer.expect_advertise().await?;
    assert_eq!(
        advertise_1.channels.len(),
        1,
        "expected 1 channel before partition"
    );
    assert_eq!(advertise_1.channels[0].topic, "/partition-test/before");
    info!(
        "phase 1 complete: viewer connected, got ServerInfo (session={:?}) and 1 channel ad",
        server_info_1.session_id
    );

    // Phase 2: Impose a full network partition.
    // Use "all" to update every netem qdisc regardless of mode (flat or
    // per-link). The "default" target only matches the ff00: handle used in
    // per-link mode and silently does nothing in flat mode.
    info!("imposing partition: 100% packet loss on all netem qdiscs");
    netem_helpers::set_netem_impairment(&container, "all", "loss 100%")?;

    // Create a second channel. The gateway will try to send an Advertise
    // message to the viewer, but the partition will prevent delivery. This
    // should eventually trigger a control write failure.
    let channel_b = ctx
        .channel_builder("/partition-test/during")
        .message_encoding("json")
        .build_raw()
        .context("create channel B")?;
    info!("created channel B during partition");

    // Give WebRTC time to detect the unresponsive peer and for the gateway's
    // flush-task to hit a write failure, which triggers `reset_participant`.
    // 10s is conservative — ICE typically detects loss within 5-8s.
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Phase 3: Lift the partition.
    info!("lifting partition: restoring default impairment");
    let default_args = netem_helpers::default_netem_args();
    netem_helpers::set_netem_impairment(&container, "all", &default_args)?;

    // Phase 4: Reconnect with the same identity.
    // The original connection is likely dead. Close it (ignoring errors) and
    // reconnect as "viewer-1".
    let close_result = viewer.close().await;
    info!("closed old viewer connection: {close_result:?}");

    let mut viewer =
        ViewerConnection::connect_with_timeout(&gw.room_name, "viewer-1", NETEM_EVENT_TIMEOUT)
            .await?;
    let server_info_2 = viewer.expect_server_info().await?;
    info!(
        "phase 4: reconnected, got fresh ServerInfo (session={:?})",
        server_info_2.session_id
    );

    // Verify that the partition triggered the reset_participant recovery path.
    assert!(
        logs_contain("resetting participant after control-plane failure"),
        "partition did not trigger reset_participant — \
         the 10s sleep may not have been long enough for WebRTC to detect the peer loss"
    );

    // The gateway should advertise ALL channels to the reconnected viewer.
    let advertise_2 = viewer.expect_advertise().await?;
    let topics: Vec<&str> = advertise_2
        .channels
        .iter()
        .map(|ch| ch.topic.as_ref())
        .collect();
    info!("advertised channels after recovery: {topics:?}");

    assert!(
        topics.contains(&"/partition-test/before"),
        "channel A (created before partition) missing from advertisements: {topics:?}"
    );
    assert!(
        topics.contains(&"/partition-test/during"),
        "channel B (created during partition) missing from advertisements: {topics:?}"
    );
    assert_eq!(
        advertise_2.channels.len(),
        2,
        "expected exactly 2 channels after recovery, got: {topics:?}"
    );

    info!(
        "partition recovery verified: viewer received ServerInfo + all {} channel ads",
        advertise_2.channels.len()
    );

    // Keep channels alive until after assertions to prevent early cleanup.
    drop(channel_a);
    drop(channel_b);

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

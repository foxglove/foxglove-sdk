//! Integration test for control stream write failure recovery under network
//! partition (FLE-384).
//!
//! Verifies that the SDK gateway recovers correctly when a network partition
//! causes control stream writes to fail. After the partition is lifted, a
//! reconnecting viewer should receive a fresh `ServerInfo` and advertisements
//! for all channels — including those created during the partition.
//!
//! These tests require the netem Docker Compose overlay:
//!   docker compose -f docker-compose.yaml -f docker-compose.netem.yml up -d --wait
//!
//! Run with: `cargo test -p remote_access_tests -- --ignored netem_partition_`

mod netem_helpers;

use std::time::Duration;

use anyhow::{Context as _, Result};
use remote_access_tests::test_helpers::{NETEM_EVENT_TIMEOUT, TestGateway, ViewerConnection};
use serial_test::serial;
use tracing::info;
use tracing_test::traced_test;

/// Verify that a network partition triggers control stream recovery.
///
/// 1. Connect a viewer and verify initial `ServerInfo` + channel advertisement.
/// 2. Impose a full partition (100% packet loss) on the default netem class.
/// 3. Create a second channel on the gateway. The gateway will attempt to send
///    an `Advertise` message, which will fail because the partition blocks
///    egress from LiveKit.
/// 4. Wait for the partition to cause write failures and (likely) a viewer
///    disconnect.
/// 5. Lift the partition, restoring connectivity.
/// 6. Reconnect the viewer and verify it receives a fresh `ServerInfo` and
///    advertisements for *both* channels — proving the gateway recovered and
///    re-advertised all state.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(netem)]
async fn netem_partition_recovery_readvertises_all_channels() -> Result<()> {
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

    // Give the partition time to cause write failures and/or disconnect.
    // WebRTC data channels need time to detect the unresponsive peer.
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Phase 3: Lift the partition.
    info!("lifting partition: restoring default impairment");
    let default_args = netem_helpers::default_netem_args();
    netem_helpers::set_netem_impairment(&container, "all", &default_args)?;

    // Phase 4: Reconnect and verify recovery.
    // The original viewer connection is likely dead. Close it (ignoring errors)
    // and establish a fresh connection.
    let close_result = viewer.close().await;
    info!("closed old viewer connection: {close_result:?}");

    let mut viewer =
        ViewerConnection::connect_with_timeout(&gw.room_name, "viewer-2", NETEM_EVENT_TIMEOUT)
            .await?;
    let server_info_2 = viewer.expect_server_info().await?;
    info!(
        "phase 4: reconnected, got fresh ServerInfo (session={:?})",
        server_info_2.session_id
    );

    // Verify that the partition triggered the reset_participant recovery path.
    // The session_id is intentionally stable (it's an API-level correlation ID,
    // not a per-session value), so we check the gateway's tracing logs instead.
    assert!(
        logs_contain("resetting participant after control-plane failure"),
        "partition did not trigger reset_participant — \
         the 10s sleep may not have been long enough for WebRTC to detect the peer loss"
    );

    // The gateway should advertise ALL channels to the new viewer.
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

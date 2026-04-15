//! Integration tests for remote access status support: publish_status and remove_status.
//!
//! Requires a local LiveKit server via `docker compose up -d`.
//! Run with: `cargo test -p remote_access_tests -- --ignored livekit_status_`

use anyhow::Result;
use foxglove::remote_access::Status;
use remote_access_tests::test_helpers::{TestGateway, TestGatewayOptions, ViewerConnection};
use serial_test::serial;
use tracing::info;
use tracing_test::traced_test;

/// Test that publish_status sends a status message to a connected viewer.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_status_publish() -> Result<()> {
    let ctx = foxglove::Context::new();
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    // Publish a warning status with an ID.
    gw.handle
        .publish_status(Status::warning("something went wrong").with_id("warn-1"));

    let status = viewer.expect_status().await?;
    info!("Status: {status:?}");
    assert_eq!(status.message, "something went wrong");
    assert_eq!(status.id.as_deref(), Some("warn-1"));
    assert_eq!(status.level, foxglove::remote_access::StatusLevel::Warning);

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that remove_status sends a removeStatus message to a connected viewer.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_status_remove() -> Result<()> {
    let ctx = foxglove::Context::new();
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    // Publish a status, then remove it.
    gw.handle
        .publish_status(Status::error("disk full").with_id("err-1"));
    let status = viewer.expect_status().await?;
    assert_eq!(status.id.as_deref(), Some("err-1"));

    gw.handle.remove_status(vec!["err-1".to_string()]);

    let remove = viewer.expect_remove_status().await?;
    info!("RemoveStatus: {remove:?}");
    assert_eq!(remove.status_ids, vec!["err-1"]);

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

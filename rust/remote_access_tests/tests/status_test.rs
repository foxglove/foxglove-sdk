//! Integration tests for remote access status support: publish_status, send_status, and
//! remove_status.
//!
//! Requires a local LiveKit server via `docker compose up -d`.
//! Run with: `cargo test -p remote_access_tests -- --ignored livekit_status_`

use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result};
use foxglove::ChannelDescriptor;
use foxglove::remote_access::{Capability, Client, ClientId, Listener, Status, StatusLevel};
use remote_access_tests::test_helpers::{
    ClientChannelDesc, TestGateway, TestGatewayOptions, ViewerConnection, poll_until,
};
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

/// Records the client IDs of client advertisements and unadvertisements.
///
/// When `gateway` is set, also answers each advertisement from inside the callback with an error
/// status sent only to the advertising client. The gateway invokes this callback while holding
/// its subscription lock.
#[derive(Default)]
struct AdvertiseListener {
    gateway: Mutex<Option<TestGateway>>,
    advertised: Mutex<Vec<ClientId>>,
    unadvertised: Mutex<Vec<ClientId>>,
}

impl AdvertiseListener {
    fn advertised(&self) -> Vec<ClientId> {
        self.advertised.lock().unwrap().clone()
    }

    fn unadvertised(&self) -> Vec<ClientId> {
        self.unadvertised.lock().unwrap().clone()
    }

    fn publish_status(&self, status: Status) {
        let gateway = self.gateway.lock().unwrap();
        gateway
            .as_ref()
            .expect("gateway should be set")
            .handle
            .publish_status(status);
    }
}

impl Listener for AdvertiseListener {
    fn on_client_advertise(&self, client: &Client, channel: &ChannelDescriptor) {
        self.advertised.lock().unwrap().push(client.id());
        if let Some(gw) = self.gateway.lock().unwrap().as_ref() {
            gw.handle.send_status(
                client.id(),
                Status::error(format!("Advertisement of {} rejected", channel.topic())),
            );
        }
    }

    fn on_client_unadvertise(&self, client: &Client, _channel: &ChannelDescriptor) {
        self.unadvertised.lock().unwrap().push(client.id());
    }
}

fn cmd_channel() -> ClientChannelDesc {
    ClientChannelDesc {
        id: 1,
        topic: "/cmd".to_string(),
        encoding: "json".to_string(),
        schema_name: String::new(),
    }
}

/// Test that send_status delivers a status message only to the targeted viewer, and is a no-op
/// for unknown and disconnected clients.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_status_send_to_single_client() -> Result<()> {
    let ctx = foxglove::Context::new();
    let listener = Arc::new(AdvertiseListener::default());
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            capabilities: vec![Capability::ClientPublish],
            ..Default::default()
        },
    )
    .await?;

    let mut viewer1 = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer1.expect_server_info().await?;
    let mut viewer2 = ViewerConnection::connect(&gw.room_name, "viewer-2").await?;
    let _server_info = viewer2.expect_server_info().await?;

    // Advertise a channel from viewer-1 to learn its client ID.
    viewer1.send_client_advertise(&[cmd_channel()]).await?;
    poll_until(|| listener.advertised().len() == 1).await;
    let viewer1_id = listener.advertised()[0];

    gw.handle.send_status(
        viewer1_id,
        Status::warning("only for viewer-1").with_id("v1"),
    );
    let status = viewer1.expect_status().await?;
    assert_eq!(status.message, "only for viewer-1");
    assert_eq!(status.id.as_deref(), Some("v1"));
    assert_eq!(status.level, StatusLevel::Warning);

    // viewer-2 didn't receive the status, so the next broadcast is the first status it sees.
    gw.handle
        .send_status(ClientId::from(u32::MAX), Status::error("nobody"));
    gw.handle.publish_status(Status::error("fence 1"));
    assert_eq!(viewer2.expect_status().await?.message, "fence 1");
    assert_eq!(viewer1.expect_status().await?.message, "fence 1");

    // Once viewer-1 is removed (which fires on_client_unadvertise), sending to it is a no-op.
    viewer1.close().await?;
    poll_until(|| listener.unadvertised() == vec![viewer1_id]).await;
    gw.handle
        .send_status(viewer1_id, Status::error("viewer-1 is gone"));
    gw.handle.publish_status(Status::error("fence 2"));
    assert_eq!(viewer2.expect_status().await?.message, "fence 2");

    viewer2.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that send_status can be called from inside the `on_client_advertise` callback, which
/// runs while the gateway holds its subscription lock, and reaches only the advertising viewer.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_status_send_from_client_advertise_callback() -> Result<()> {
    let ctx = foxglove::Context::new();
    let listener = Arc::new(AdvertiseListener::default());
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            listener: Some(listener.clone()),
            capabilities: vec![Capability::ClientPublish],
            ..Default::default()
        },
    )
    .await?;
    let room_name = gw.room_name.clone();
    *listener.gateway.lock().unwrap() = Some(gw);

    let mut viewer1 = ViewerConnection::connect(&room_name, "viewer-1").await?;
    let _server_info = viewer1.expect_server_info().await?;
    let mut viewer2 = ViewerConnection::connect(&room_name, "viewer-2").await?;
    let _server_info = viewer2.expect_server_info().await?;

    viewer1.send_client_advertise(&[cmd_channel()]).await?;
    let status = viewer1.expect_status().await?;
    assert_eq!(status.message, "Advertisement of /cmd rejected");
    assert_eq!(status.level, StatusLevel::Error);
    assert_eq!(listener.advertised().len(), 1);

    // viewer-2 didn't receive the status, so the next broadcast is the first status it sees.
    listener.publish_status(Status::error("fence"));
    assert_eq!(viewer2.expect_status().await?.message, "fence");

    viewer1.close().await?;
    viewer2.close().await?;
    let gw = listener
        .gateway
        .lock()
        .unwrap()
        .take()
        .context("gateway should be set")?;
    gw.stop().await?;
    Ok(())
}

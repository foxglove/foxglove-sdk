//! Gateway shutdown when the LiveKit signal link is dead.
//!
//! LiveKit's `Room::close()` can block indefinitely when the signal link wedges (DB-1714), so
//! `RemoteAccessSession::close` bounds its wait and lets the close finish in the background.
//! This test covers the resulting guarantee: gateway shutdown completes on a schedule we
//! control, no matter what the signal link does.
//!
//! It deliberately does not assert that the close hangs. That is upstream behavior, and an
//! upstream fix should not turn this red; the bound holds either way.
//!
//! Requires a local LiveKit server via `docker compose up -d`.
//! Run with: `cargo test -p remote_access_tests -- --ignored livekit_room_close`

use std::time::Duration;

use anyhow::Result;
use foxglove::remote_access::ConnectionStatus;
use remote_access_tests::blackhole_proxy::BlackholeProxy;
use remote_access_tests::livekit_token;
use remote_access_tests::test_helpers::{
    EVENT_TIMEOUT, TestGateway, TestGatewayOptions, ViewerConnection, poll_until,
};
use serial_test::serial;
use tracing::info;
use tracing_test::traced_test;

/// Ceiling on gateway shutdown with a dead signal link. `ROOM_CLOSE_TIMEOUT` in the SDK is 20s;
/// the rest covers the teardown either side of the room close.
const SHUTDOWN_BOUND: Duration = Duration::from_secs(35);

/// Blackholing the gateway's signal link must not wedge its shutdown: the room close is bounded
/// and the runner still returns.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_room_close_is_bounded_when_signal_blackholed() -> Result<()> {
    // Only the gateway goes through the proxy. The viewer talks to the dev server directly, so
    // it keeps observing the room after we cut the gateway's link.
    let proxy = BlackholeProxy::start(livekit_host_port()).await?;
    let (room_name, mock) =
        TestGateway::prepare_with_livekit_url(format!("http://{}", proxy.addr())).await;

    let ctx = foxglove::Context::new();
    let gw = TestGateway::start_with_mock(&ctx, room_name, mock, TestGatewayOptions::default())?;
    poll_until(|| gw.handle.connection_status() == ConnectionStatus::Connected).await;

    // A viewer that reaches ServerInfo proves the gateway is fully joined, with a working data
    // path, before we cut the link.
    let (viewer, _server_info, _advertise) = ViewerConnection::connect_and_await_startup(
        &gw.room_name,
        "viewer-1",
        false,
        EVENT_TIMEOUT,
    )
    .await?;
    info!("gateway joined and serving; blackholing its signal link");

    proxy.blackhole();

    let elapsed = gw.stop_with_timeout(SHUTDOWN_BOUND).await?;
    info!("gateway stopped after {elapsed:?} with a dead signal link");

    viewer.close().await?;
    Ok(())
}

/// The LiveKit dev server's `host:port`, as a plain TCP address for the proxy.
fn livekit_host_port() -> String {
    let url = livekit_token::livekit_url();
    let rest = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(&url)
        .trim_end_matches('/');
    if rest.contains(':') {
        rest.to_string()
    } else {
        format!("{rest}:7880")
    }
}

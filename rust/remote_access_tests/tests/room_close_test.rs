//! Reproduces the LiveKit room-close hang seen in the field (DB-1284): after the
//! signal link is blackholed, `Room::close()` never returns, so the device stays
//! in the room even though the SDK believes it has disconnected.
//!
//! Requires a local LiveKit server via `docker compose up -d`.
//! Run with: `cargo test -p remote_access_tests -- --ignored room_close_`

use std::time::Duration;

use anyhow::{Result, bail};
use livekit::{ConnectionState, Room, RoomEvent, RoomOptions};
use remote_access_tests::blackhole_proxy::BlackholeProxy;
use remote_access_tests::livekit_token;
use remote_access_tests::test_helpers::unique_id;
use serial_test::serial;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::info;

/// How long to wait for the signal ping timeout to trip after the blackhole.
const RECONNECT_TIMEOUT: Duration = Duration::from_secs(90);
/// How long we allow `Room::close()` to take before calling it hung.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Blackholing the signal link wedges `Room::close()`: the LiveKit signal
/// client's teardown blocks forever, so the room is never actually left.
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn room_close_hangs_after_signal_blackhole() -> Result<()> {
    init_logging();
    let proxy = BlackholeProxy::start(livekit_host_port()).await?;
    let url = format!("ws://{}", proxy.addr());
    let room_name = format!("test-room-{}", unique_id());
    let token = livekit_token::generate_token(&room_name, "device")?;

    let (room, mut events) = Room::connect(&url, &token, RoomOptions::default()).await?;
    info!("connected to room via proxy at {url}");

    proxy.blackhole();
    let started = tokio::time::Instant::now();
    wait_for_state(
        &mut events,
        ConnectionState::Reconnecting,
        RECONNECT_TIMEOUT,
    )
    .await?;
    info!("room entered Reconnecting after {:?}", started.elapsed());

    let started = tokio::time::Instant::now();
    match tokio::time::timeout(CLOSE_TIMEOUT, room.close()).await {
        Ok(result) => {
            info!("room.close() returned after {:?}", started.elapsed());
            result?;
            Ok(())
        }
        Err(_) => bail!("room.close() hung for {CLOSE_TIMEOUT:?} after the signal link died"),
    }
}

/// Control: with the link intact, `Room::close()` returns promptly. Guards
/// against the blackhole test passing for an unrelated reason.
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn room_close_returns_promptly_when_connected() -> Result<()> {
    init_logging();
    let proxy = BlackholeProxy::start(livekit_host_port()).await?;
    let url = format!("ws://{}", proxy.addr());
    let room_name = format!("test-room-{}", unique_id());
    let token = livekit_token::generate_token(&room_name, "device")?;

    let (room, _events) = Room::connect(&url, &token, RoomOptions::default()).await?;

    let started = tokio::time::Instant::now();
    tokio::time::timeout(CLOSE_TIMEOUT, room.close())
        .await
        .map_err(|_| anyhow::anyhow!("room.close() hung on a healthy connection"))??;
    info!("room.close() returned after {:?}", started.elapsed());
    Ok(())
}

/// Installs a subscriber that also captures LiveKit's `log`-crate records, so
/// the reconnect path's own tracing is visible in test output.
fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,livekit=debug"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_test_writer()
        .try_init();
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

/// Waits for a [`RoomEvent::ConnectionStateChanged`] matching `expected`.
async fn wait_for_state(
    events: &mut UnboundedReceiver<RoomEvent>,
    expected: ConnectionState,
    timeout: Duration,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            bail!("timed out waiting for connection state {expected:?}");
        }
        match tokio::time::timeout(remaining, events.recv()).await {
            Ok(Some(RoomEvent::ConnectionStateChanged(state))) => {
                info!("connection state changed: {state:?}");
                if state == expected {
                    return Ok(());
                }
            }
            Ok(Some(_)) => {}
            Ok(None) => bail!("room event stream ended while waiting for {expected:?}"),
            Err(_) => bail!("timed out waiting for connection state {expected:?}"),
        }
    }
}

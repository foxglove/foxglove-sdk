//! Integration test that validates authentication against the Foxglove platform.
//!
//! Requires `FOXGLOVE_API_KEY` to be set (e.g. via `.env`).
//! Run with: `cargo test -p remote_access_tests -- --ignored auth_`

use std::time::Duration;

use anyhow::{Context, Result};
use remote_access_tests::config::Config;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use tracing::info;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Device {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceToken {
    id: String,
    token: String,
}

/// Creates a device via the Foxglove platform API.
async fn create_device(client: &reqwest::Client, api_url: &str, api_key: &str) -> Result<Device> {
    let resp = client
        .post(format!("{api_url}/v1/devices"))
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .header(CONTENT_TYPE, "application/json")
        .body(r#"{"name":"ra-integration-test"}"#)
        .send()
        .await
        .context("POST /v1/devices")?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("create device failed ({status}): {body}");
    }
    serde_json::from_str(&body).context("parse device response")
}

/// Creates a device token via the Foxglove platform API.
async fn create_device_token(
    client: &reqwest::Client,
    api_url: &str,
    api_key: &str,
    device_id: &str,
) -> Result<DeviceToken> {
    let resp = client
        .post(format!("{api_url}/v1/device-tokens"))
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .header(CONTENT_TYPE, "application/json")
        .body(serde_json::to_string(&serde_json::json!({
            "deviceId": device_id,
        }))?)
        .send()
        .await
        .context("POST /v1/device-tokens")?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("create device token failed ({status}): {body}");
    }
    serde_json::from_str(&body).context("parse device token response")
}

/// Deletes a device token via the Foxglove platform API.
async fn delete_device_token(
    client: &reqwest::Client,
    api_url: &str,
    api_key: &str,
    token_id: &str,
) -> Result<()> {
    let resp = client
        .delete(format!("{api_url}/v1/device-tokens/{token_id}"))
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .send()
        .await
        .context("DELETE /v1/device-tokens")?;
    if !resp.status().is_success() {
        let body = resp.text().await?;
        anyhow::bail!("delete device token failed: {body}");
    }
    Ok(())
}

/// Deletes a device via the Foxglove platform API.
async fn delete_device(
    client: &reqwest::Client,
    api_url: &str,
    api_key: &str,
    device_id: &str,
) -> Result<()> {
    let resp = client
        .delete(format!("{api_url}/v1/devices/{device_id}"))
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .send()
        .await
        .context("DELETE /v1/devices")?;
    if !resp.status().is_success() {
        let body = resp.text().await?;
        anyhow::bail!("delete device failed: {body}");
    }
    Ok(())
}

/// Test that we can provision a device and device token, then start a RemoteAccessSink
/// that successfully authenticates and begins running.
///
/// TODO: This test currently only validates that the auth + connect flow doesn't panic or
/// hang. It cannot verify that the LiveKit connection actually succeeded because the
/// Foxglove platform controls room creation and token issuance — there's no way to
/// independently join the room from the test. Once RemoteAccessSink exposes a connection
/// status callback or similar API, this test should assert on successful connection.
#[ignore]
#[tokio::test]
async fn auth_remote_access_connection() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let config = Config::get();
    let client = reqwest::Client::new();

    // Create a device and device token via the Foxglove platform API.
    let device = create_device(&client, &config.foxglove_api_url, &config.foxglove_api_key)
        .await
        .context("create device")?;
    info!("created device: {}", device.id);

    let device_token = create_device_token(
        &client,
        &config.foxglove_api_url,
        &config.foxglove_api_key,
        &device.id,
    )
    .await
    .context("create device token")?;
    info!("created device token: {}", device_token.id);

    // Start a RemoteAccessSink pointed at the platform.
    let handle = foxglove::RemoteAccessSink::new()
        .name("auth-integration-test")
        .device_token(&device_token.token)
        .foxglove_api_url(&config.foxglove_api_url)
        .start()
        .context("start RemoteAccessSink")?;

    // Give it time to connect and authenticate.
    // The sink authenticates via device-info, then fetches RTC credentials.
    // We wait long enough for the auth flow to complete.
    tokio::time::sleep(Duration::from_secs(10)).await;
    info!("stopping remote access sink after auth test window");

    // Stop the sink.
    let runner = handle.stop();
    tokio::time::timeout(Duration::from_secs(10), runner)
        .await
        .context("timeout waiting for sink to stop")?
        .context("sink runner panicked")?;

    // Cleanup: delete the device token and device.
    delete_device_token(
        &client,
        &config.foxglove_api_url,
        &config.foxglove_api_key,
        &device_token.id,
    )
    .await
    .context("delete device token")?;

    delete_device(
        &client,
        &config.foxglove_api_url,
        &config.foxglove_api_key,
        &device.id,
    )
    .await
    .context("delete device")?;

    info!("auth test completed successfully");
    Ok(())
}

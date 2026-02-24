//! Integration test that validates authentication against the Foxglove platform.
//!
//! Requires `FOXGLOVE_API_KEY` to be set (e.g. via `.env`).
//! Run with: `cargo test -p remote_access_tests -- --ignored auth_`

use std::time::Duration;

use anyhow::{Context, Result};
use remote_access_tests::config::Config;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use tracing::{info, warn};
use tracing_test::traced_test;

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
async fn create_device(
    client: &reqwest::Client,
    api_url: &str,
    api_key: &str,
    name: &str,
) -> Result<Device> {
    let resp = client
        .post(format!("{api_url}/v1/devices"))
        .header(AUTHORIZATION, format!("Bearer {api_key}"))
        .header(CONTENT_TYPE, "application/json")
        .body(serde_json::to_string(&serde_json::json!({ "name": name }))?)
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

/// Test that we can provision a device and device token, then start a Gateway
/// that successfully authenticates and begins running.
///
/// TODO: This test currently only validates that the auth + connect flow doesn't panic or
/// hang. It cannot verify that the LiveKit connection actually succeeded because the
/// Foxglove platform controls room creation and token issuance — there's no way to
/// independently join the room from the test. Once Gateway exposes a connection
/// status callback or similar API, this test should assert on successful connection.
#[traced_test]
#[ignore]
#[tokio::test]
async fn auth_remote_access_connection() -> Result<()> {
    let config = Config::get();
    let client = reqwest::Client::new();

    // Create a device and device token via the Foxglove platform API.
    let device_name = format!("ra-integration-test-{}", unique_id());
    let device = retry(3, || async {
        create_device(
            &client,
            &config.foxglove_api_url,
            &config.foxglove_api_key,
            &device_name,
        )
        .await
    })
    .await
    .context("create device")?;
    info!("created device: {}", device.id);

    let device_token = retry(3, || async {
        create_device_token(
            &client,
            &config.foxglove_api_url,
            &config.foxglove_api_key,
            &device.id,
        )
        .await
    })
    .await
    .context("create device token")?;
    info!("created device token: {}", device_token.id);

    // Run the test, capturing the result so cleanup always executes.
    let test_result = run_auth_test(config, &device_token.token).await;

    // Always clean up platform resources regardless of test outcome.
    // Run both deletions unconditionally so a token deletion failure doesn't leak the device.
    let token_result = retry(3, || async {
        delete_device_token(
            &client,
            &config.foxglove_api_url,
            &config.foxglove_api_key,
            &device_token.id,
        )
        .await
    })
    .await
    .context("delete device token");

    let device_result = retry(3, || async {
        delete_device(
            &client,
            &config.foxglove_api_url,
            &config.foxglove_api_key,
            &device.id,
        )
        .await
    })
    .await
    .context("delete device");

    // Return the test result first; if it passed, return cleanup errors.
    test_result?;
    token_result?;
    device_result?;
    info!("auth test completed successfully");
    Ok(())
}

async fn run_auth_test(config: &Config, token: &str) -> Result<()> {
    let handle = foxglove::remote_access::Gateway::new()
        .name("auth-integration-test")
        .device_token(token)
        .foxglove_api_url(&config.foxglove_api_url)
        .start()
        .context("start Gateway")?;

    // Give it time to connect and authenticate.
    // The sink authenticates via device-info, then fetches RTC credentials.
    // We wait long enough for the auth flow to complete.
    tokio::time::sleep(Duration::from_secs(10)).await;
    info!("stopping remote access sink after auth test window");

    let runner = handle.stop();
    tokio::time::timeout(Duration::from_secs(10), runner)
        .await
        .context("timeout waiting for sink to stop")?
        .context("sink runner panicked")?;

    Ok(())
}

/// Retries an async operation with exponential backoff on failure.
async fn retry<F, Fut, T>(max_attempts: u32, f: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut last_err = None;
    for attempt in 1..=max_attempts {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                if attempt < max_attempts {
                    let delay = Duration::from_secs(2u64.pow(attempt - 1));
                    warn!("attempt {attempt}/{max_attempts} failed: {e:#}; retrying in {delay:?}");
                    tokio::time::sleep(delay).await;
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap())
}

fn unique_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    format!("{nanos:x}-{pid:x}")
}

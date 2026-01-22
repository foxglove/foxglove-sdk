#![allow(dead_code, unused_imports)]

mod client;
mod credentials_provider;
mod device;
mod types;

use client::FoxgloveApiClient;
pub(crate) use credentials_provider::{CredentialsError, CredentialsProvider, RtcCredentials};
pub(crate) use device::{Device, DeviceBuilder, DeviceBuilderFromToken};

#[cfg(test)]
mod tests {
    use super::*;

    // Run with:
    // FOXGLOVE_DEVICE_TOKEN=<token> cargo test -p foxglove --features agent test_fetch_rtc_credentials -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "requires FOXGLOVE_DEVICE_TOKEN environment variable"]
    async fn test_fetch_rtc_credentials() {
        let device_token =
            std::env::var("FOXGLOVE_DEVICE_TOKEN").expect("FOXGLOVE_DEVICE_TOKEN must be set");

        let api_url = std::env::var("FOXGLOVE_API_URL").ok();

        println!("API URL: {}", api_url.as_deref().unwrap_or(DEFAULT_API_URL));
        println!();

        println!("Fetching device info...");
        let mut builder = DeviceBuilderFromToken::new(&device_token);
        if let Some(url) = api_url {
            builder = builder.base_url(url);
        }
        let device = builder.build().await.expect("Failed to build device");

        println!("Device ID: {}", device.id());
        println!("Device Name: {}", device.name());
        println!("Project ID: {}", device.project_id());
        println!();

        let provider = CredentialsProvider::new(device);

        println!("Fetching RTC credentials...");
        let credentials = provider
            .load_credentials()
            .await
            .expect("Failed to load credentials");

        println!("Success!");
        println!("  URL: {}", credentials.url);
        println!(
            "  Token: {}...",
            &credentials.token[..50.min(credentials.token.len())]
        );
    }
}

//! Integration tests for remote access service advertisement, service call
//! request/response, and error handling.
//!
//! Requires a local LiveKit server via `docker compose up -d`.
//! Run with: `cargo test -p remote_access_tests -- --ignored livekit_service_`

use std::borrow::Cow;

use anyhow::{Context as _, Result};
use foxglove::Schema;
use foxglove::protocol::v2::client::ServiceCallRequest;
use foxglove::protocol::v2::server::{ServerMessage, server_info};
use foxglove::remote_access::{Service, ServiceSchema};
use remote_access_tests::test_helpers::{TestGateway, ViewerConnection};
use serial_test::serial;
use tracing::info;
use tracing_test::traced_test;

/// Helper: creates a simple echo service that returns the request payload as the response.
fn echo_service() -> Service {
    let schema = ServiceSchema::new("EchoService")
        .with_request("json", Schema::new("EchoRequest", "jsonschema", b"{}"))
        .with_response("json", Schema::new("EchoResponse", "jsonschema", b"{}"));
    Service::builder("echo", schema).handler_fn(|req| {
        let payload = req.payload().to_vec();
        Ok::<_, String>(payload)
    })
}

/// Helper: creates a service that always returns an error.
fn failing_service() -> Service {
    let schema = ServiceSchema::new("FailService")
        .with_request("json", Schema::new("FailRequest", "jsonschema", b"{}"))
        .with_response("json", Schema::new("FailResponse", "jsonschema", b"{}"));
    Service::builder("fail", schema)
        .handler_fn(|_req| Err::<Vec<u8>, _>("handler error".to_string()))
}

// ===========================================================================
// Tests
// ===========================================================================

/// Test that a viewer receives an AdvertiseServices message when connecting to a
/// gateway that has services registered.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_service_viewer_receives_advertise_services() -> Result<()> {
    let ctx = foxglove::Context::new();
    let services = vec![echo_service()];
    let gw = TestGateway::start_with_services(&ctx, services).await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let server_info = viewer.expect_server_info().await?;
    info!("ServerInfo: {server_info:?}");

    // The gateway should advertise the "services" capability.
    assert!(
        server_info
            .capabilities
            .iter()
            .any(|c| c == &server_info::Capability::Services),
        "server_info should include 'services' capability, got: {:?}",
        server_info.capabilities
    );

    let adv_services = viewer.expect_advertise_services().await?;
    info!("AdvertiseServices: {adv_services:?}");

    assert_eq!(adv_services.services.len(), 1);
    assert_eq!(adv_services.services[0].name, "echo");
    assert_eq!(adv_services.services[0].r#type, "EchoService");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that a viewer receives AdvertiseServices for multiple services.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_service_viewer_receives_multiple_services() -> Result<()> {
    let ctx = foxglove::Context::new();
    let services = vec![echo_service(), failing_service()];
    let gw = TestGateway::start_with_services(&ctx, services).await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;
    let adv_services = viewer.expect_advertise_services().await?;

    assert_eq!(adv_services.services.len(), 2);
    let names: Vec<&str> = adv_services
        .services
        .iter()
        .map(|s| s.name.as_ref())
        .collect();
    assert!(names.contains(&"echo"), "expected 'echo' service");
    assert!(names.contains(&"fail"), "expected 'fail' service");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that a successful service call returns a ServiceCallResponse with the
/// expected payload.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_service_call_returns_response() -> Result<()> {
    let ctx = foxglove::Context::new();
    let services = vec![echo_service()];
    let gw = TestGateway::start_with_services(&ctx, services).await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;
    let adv_services = viewer.expect_advertise_services().await?;

    let service_id = adv_services.services[0].id;
    let call_id = 42u32;
    let payload = br#"{"hello":"world"}"#;

    let req = ServiceCallRequest {
        service_id,
        call_id,
        encoding: Cow::Borrowed("json"),
        payload: Cow::Borrowed(payload),
    };
    viewer.send_service_call_request(&req).await?;

    let response = viewer.expect_service_call_response().await?;
    info!("ServiceCallResponse: {response:?}");

    assert_eq!(response.service_id, service_id);
    assert_eq!(response.call_id, call_id);
    assert_eq!(response.encoding, "json");
    assert_eq!(response.payload.as_ref(), payload);

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that calling an unknown service ID returns a ServiceCallFailure.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_service_call_unknown_service_returns_failure() -> Result<()> {
    let ctx = foxglove::Context::new();
    let services = vec![echo_service()];
    let gw = TestGateway::start_with_services(&ctx, services).await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;
    let _adv_services = viewer.expect_advertise_services().await?;

    let req = ServiceCallRequest {
        service_id: 99999, // non-existent
        call_id: 1,
        encoding: Cow::Borrowed("json"),
        payload: Cow::Borrowed(b"{}"),
    };
    viewer.send_service_call_request(&req).await?;

    let failure = viewer.expect_service_call_failure().await?;
    info!("ServiceCallFailure: {failure:?}");

    assert_eq!(failure.service_id, 99999);
    assert_eq!(failure.call_id, 1);
    assert!(
        failure.message.contains("Unknown service"),
        "expected 'Unknown service' message, got: {}",
        failure.message
    );

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that calling a service with an unsupported encoding returns a ServiceCallFailure.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_service_call_unsupported_encoding_returns_failure() -> Result<()> {
    let ctx = foxglove::Context::new();
    let services = vec![echo_service()];
    let gw = TestGateway::start_with_services(&ctx, services).await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;
    let adv_services = viewer.expect_advertise_services().await?;

    let service_id = adv_services.services[0].id;
    let req = ServiceCallRequest {
        service_id,
        call_id: 2,
        encoding: Cow::Borrowed("protobuf"), // not "json"
        payload: Cow::Borrowed(b"{}"),
    };
    viewer.send_service_call_request(&req).await?;

    let failure = viewer.expect_service_call_failure().await?;
    info!("ServiceCallFailure: {failure:?}");

    assert_eq!(failure.service_id, service_id);
    assert_eq!(failure.call_id, 2);
    assert!(
        failure.message.contains("Unsupported encoding"),
        "expected 'Unsupported encoding' message, got: {}",
        failure.message
    );

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that a handler error results in a ServiceCallFailure sent to the viewer.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_service_call_handler_error_returns_failure() -> Result<()> {
    let ctx = foxglove::Context::new();
    let services = vec![failing_service()];
    let gw = TestGateway::start_with_services(&ctx, services).await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;
    let adv_services = viewer.expect_advertise_services().await?;

    let service_id = adv_services.services[0].id;
    let req = ServiceCallRequest {
        service_id,
        call_id: 3,
        encoding: Cow::Borrowed("json"),
        payload: Cow::Borrowed(b"{}"),
    };
    viewer.send_service_call_request(&req).await?;

    let failure = viewer.expect_service_call_failure().await?;
    info!("ServiceCallFailure: {failure:?}");

    assert_eq!(failure.service_id, service_id);
    assert_eq!(failure.call_id, 3);
    assert!(
        failure.message.contains("handler error"),
        "expected 'handler error' message, got: {}",
        failure.message
    );

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that a gateway without services does not send AdvertiseServices and does
/// not include the "services" capability.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_service_no_services_no_advertisement() -> Result<()> {
    let ctx = foxglove::Context::new();
    // Create a channel so there's an Advertise message after ServerInfo.
    let _channel = ctx
        .channel_builder("/test")
        .message_encoding("json")
        .build_raw()
        .context("create channel")?;

    let gw = TestGateway::start(&ctx).await?;
    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;

    let server_info = viewer.expect_server_info().await?;
    assert!(
        !server_info
            .capabilities
            .iter()
            .any(|c| c == &server_info::Capability::Services),
        "server_info should NOT include 'services' capability when no services are registered"
    );

    // Next message should be Advertise (for the channel), not AdvertiseServices.
    let msg = viewer.frame_reader.next_server_message().await?;
    assert!(
        matches!(msg, ServerMessage::Advertise(_)),
        "expected Advertise, got: {msg:?}"
    );

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

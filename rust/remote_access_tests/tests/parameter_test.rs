//! Integration tests for remote access parameter support: get, set, subscribe,
//! unsubscribe, and publish_parameter_values.
//!
//! Requires a local LiveKit server via `docker compose up -d`.
//! Run with: `cargo test -p remote_access_tests -- --ignored livekit_parameter_`

use std::sync::{Arc, Mutex};

use anyhow::Result;
use foxglove::protocol::v2::server::server_info;
use foxglove::remote_access::{Capability, Listener, Parameter};
use remote_access_tests::test_helpers::{TestGateway, TestGatewayOptions, ViewerConnection};
use serial_test::serial;
use tracing::info;
use tracing_test::traced_test;

// ---------------------------------------------------------------------------
// Mock listener that records parameter callbacks
// ---------------------------------------------------------------------------

/// A mock [`Listener`] that handles parameter get/set requests and records
/// subscribe/unsubscribe callbacks.
struct ParameterListener {
    /// Parameters returned by `on_get_parameters`. Set by the test before sending requests.
    stored_parameters: Mutex<Vec<Parameter>>,
    /// Records parameter names from subscribe callbacks.
    subscribed: Mutex<Vec<Vec<String>>>,
    /// Records parameter names from unsubscribe callbacks.
    unsubscribed: Mutex<Vec<Vec<String>>>,
}

impl ParameterListener {
    fn new(initial_parameters: Vec<Parameter>) -> Self {
        Self {
            stored_parameters: Mutex::new(initial_parameters),
            subscribed: Mutex::new(Vec::new()),
            unsubscribed: Mutex::new(Vec::new()),
        }
    }

    fn take_subscribed(&self) -> Vec<Vec<String>> {
        std::mem::take(&mut *self.subscribed.lock().unwrap())
    }

    fn take_unsubscribed(&self) -> Vec<Vec<String>> {
        std::mem::take(&mut *self.unsubscribed.lock().unwrap())
    }
}

impl Listener for ParameterListener {
    fn on_get_parameters(
        &self,
        _client: &foxglove::remote_access::Client,
        param_names: Vec<String>,
        _request_id: Option<&str>,
    ) -> Vec<Parameter> {
        let params = self.stored_parameters.lock().unwrap();
        if param_names.is_empty() {
            params.clone()
        } else {
            params
                .iter()
                .filter(|p| param_names.contains(&p.name))
                .cloned()
                .collect()
        }
    }

    fn on_set_parameters(
        &self,
        _client: &foxglove::remote_access::Client,
        parameters: Vec<Parameter>,
        _request_id: Option<&str>,
    ) -> Vec<Parameter> {
        let mut stored = self.stored_parameters.lock().unwrap();
        for param in &parameters {
            if let Some(existing) = stored.iter_mut().find(|p| p.name == param.name) {
                *existing = param.clone();
            } else {
                stored.push(param.clone());
            }
        }
        stored.clone()
    }

    fn on_parameters_subscribe(&self, param_names: Vec<String>) {
        self.subscribed.lock().unwrap().push(param_names);
    }

    fn on_parameters_unsubscribe(&self, param_names: Vec<String>) {
        self.unsubscribed.lock().unwrap().push(param_names);
    }
}

// ===========================================================================
// Tests
// ===========================================================================

/// Test that the server info advertises the parameters capabilities.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_server_info_capabilities() -> Result<()> {
    let ctx = foxglove::Context::new();
    let listener = Arc::new(ParameterListener::new(vec![]));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            listener: Some(listener),
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let server_info = viewer.expect_server_info().await?;
    info!("ServerInfo: {server_info:?}");

    assert!(
        server_info
            .capabilities
            .contains(&server_info::Capability::Parameters),
        "server_info should include 'parameters' capability"
    );
    assert!(
        server_info
            .capabilities
            .contains(&server_info::Capability::ParametersSubscribe),
        "server_info should include 'parametersSubscribe' capability"
    );

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test GetParameters round-trip: viewer sends a GetParameters request and
/// receives a ParameterValues response.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_get_parameters() -> Result<()> {
    let ctx = foxglove::Context::new();
    let params = vec![
        Parameter::string("foo", "hello"),
        Parameter::float64("bar", 42.0),
    ];
    let listener = Arc::new(ParameterListener::new(params));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            listener: Some(listener),
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    // Request specific parameters.
    viewer
        .send_get_parameters_with_id(&["foo"], "req-1")
        .await?;
    let response = viewer.expect_parameter_values().await?;
    info!("ParameterValues: {response:?}");

    assert_eq!(response.id.as_deref(), Some("req-1"));
    assert_eq!(response.parameters.len(), 1);
    assert_eq!(response.parameters[0].name, "foo");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test SetParameters round-trip: viewer sends a SetParameters request and
/// receives the updated parameters back.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_set_parameters() -> Result<()> {
    let ctx = foxglove::Context::new();
    let listener = Arc::new(ParameterListener::new(vec![Parameter::string(
        "color", "red",
    )]));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            listener: Some(listener),
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    // Set a parameter and expect the response.
    viewer
        .send_set_parameters_with_id(vec![Parameter::string("color", "blue")], "set-1")
        .await?;
    let response = viewer.expect_parameter_values().await?;
    info!("ParameterValues: {response:?}");

    assert_eq!(response.id.as_deref(), Some("set-1"));
    assert!(
        response.parameters.iter().any(|p| p.name == "color"),
        "response should include the 'color' parameter"
    );

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test subscribe/unsubscribe and publish_parameter_values: a subscribed viewer
/// receives parameter updates, and unsubscribing stops delivery.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_subscribe_and_publish() -> Result<()> {
    let ctx = foxglove::Context::new();
    let listener = Arc::new(ParameterListener::new(vec![]));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            listener: Some(listener.clone()),
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    // Subscribe to parameter updates.
    viewer
        .send_subscribe_parameter_updates(&["speed", "mode"])
        .await?;

    // Give the gateway time to process the subscription.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify the listener was notified of the subscription.
    let subscribed = listener.take_subscribed();
    assert!(
        !subscribed.is_empty(),
        "listener should have received on_parameters_subscribe"
    );

    // Publish parameter values from the gateway handle.
    gw.handle
        .publish_parameter_values(vec![Parameter::float64("speed", 99.0)]);

    // The subscribed viewer should receive the update.
    let response = viewer.expect_parameter_values().await?;
    info!("ParameterValues after publish: {response:?}");
    assert_eq!(response.parameters.len(), 1);
    assert_eq!(response.parameters[0].name, "speed");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that when viewer A sets a parameter while viewer B is subscribed, B
/// receives ALL parameters (not just the one that was set) with no request ID.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_set_notifies_subscriber_with_all_params() -> Result<()> {
    let ctx = foxglove::Context::new();
    let listener = Arc::new(ParameterListener::new(vec![
        Parameter::string("color", "red"),
        Parameter::float64("size", 10.0),
    ]));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            listener: Some(listener),
            ..Default::default()
        },
    )
    .await?;

    // Viewer B subscribes to all parameters.
    let mut viewer_b = ViewerConnection::connect(&gw.room_name, "viewer-b").await?;
    let _server_info_b = viewer_b.expect_server_info().await?;
    viewer_b
        .send_subscribe_parameter_updates(&["color", "size"])
        .await?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Viewer A sets a parameter (no subscription).
    let mut viewer_a = ViewerConnection::connect(&gw.room_name, "viewer-a").await?;
    let _server_info_a = viewer_a.expect_server_info().await?;
    viewer_a
        .send_set_parameters_with_id(vec![Parameter::string("color", "blue")], "set-a")
        .await?;

    // Viewer A gets the response with the request ID.
    let response_a = viewer_a.expect_parameter_values().await?;
    assert_eq!(response_a.id.as_deref(), Some("set-a"));

    // Viewer B should receive a notification with ALL parameters and NO request ID.
    let response_b = viewer_b.expect_parameter_values().await?;
    info!("Viewer B received: {response_b:?}");
    assert!(
        response_b.id.is_none(),
        "subscriber notification should have no request ID"
    );
    assert!(
        response_b.parameters.len() >= 2,
        "subscriber should receive all parameters, got {}",
        response_b.parameters.len()
    );

    viewer_a.close().await?;
    viewer_b.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that `publish_parameter_values` filters by subscription: a viewer only
/// receives parameters it subscribed to.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_publish_filters_by_subscription() -> Result<()> {
    let ctx = foxglove::Context::new();
    let listener = Arc::new(ParameterListener::new(vec![]));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            listener: Some(listener),
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    // Subscribe to only "alpha".
    viewer.send_subscribe_parameter_updates(&["alpha"]).await?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Publish two parameters: "alpha" and "beta".
    gw.handle.publish_parameter_values(vec![
        Parameter::float64("alpha", 1.0),
        Parameter::float64("beta", 2.0),
    ]);

    // Viewer should only receive "alpha".
    let response = viewer.expect_parameter_values().await?;
    info!("Filtered response: {response:?}");
    assert_eq!(response.parameters.len(), 1);
    assert_eq!(response.parameters[0].name, "alpha");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Test that unsubscribing stops delivery of parameter updates.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_unsubscribe_stops_delivery() -> Result<()> {
    let ctx = foxglove::Context::new();
    let listener = Arc::new(ParameterListener::new(vec![]));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            listener: Some(listener.clone()),
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    // Subscribe, publish, and confirm receipt.
    viewer.send_subscribe_parameter_updates(&["temp"]).await?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    gw.handle
        .publish_parameter_values(vec![Parameter::float64("temp", 20.0)]);
    let response = viewer.expect_parameter_values().await?;
    assert_eq!(response.parameters.len(), 1);

    // Unsubscribe.
    viewer.send_unsubscribe_parameter_updates(&["temp"]).await?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Verify the listener was notified of the unsubscription.
    let unsubscribed = listener.take_unsubscribed();
    assert!(
        !unsubscribed.is_empty(),
        "listener should have received on_parameters_unsubscribe"
    );

    // Publish again — viewer should NOT receive this.
    gw.handle
        .publish_parameter_values(vec![Parameter::float64("temp", 30.0)]);

    // Wait briefly, then verify no message was received by trying to read with a short timeout.
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        viewer.expect_parameter_values(),
    )
    .await;
    assert!(
        result.is_err(),
        "should not receive parameter values after unsubscribing"
    );

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

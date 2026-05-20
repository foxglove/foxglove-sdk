//! Integration tests for remote access parameter support: get, set, subscribe,
//! unsubscribe, and publish_parameter_values.
//!
//! Requires a local LiveKit server via `docker compose up -d`.
//! Run with: `cargo test -p remote_access_tests -- --ignored livekit_parameter_`

use std::sync::{Arc, Mutex};

use anyhow::Result;
use foxglove::protocol::v2::server::server_info;
use foxglove::remote_access::{
    AnyClient, Capability, GetParametersResponder, Listener, Parameter, ParameterHandler,
    SetParametersResponder,
};
use remote_access_tests::test_helpers::{
    TestGateway, TestGatewayOptions, ViewerConnection, poll_until,
};
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
    /// Records parameter names from `on_get_parameters` calls.
    get_calls: Mutex<Vec<Vec<String>>>,
    /// Records parameters from `on_set_parameters` calls.
    set_calls: Mutex<Vec<Vec<Parameter>>>,
    /// Records parameter names from subscribe callbacks.
    subscribed: Mutex<Vec<Vec<String>>>,
    /// Records parameter names from unsubscribe callbacks.
    unsubscribed: Mutex<Vec<Vec<String>>>,
}

impl ParameterListener {
    fn new(initial_parameters: Vec<Parameter>) -> Self {
        Self {
            stored_parameters: Mutex::new(initial_parameters),
            get_calls: Mutex::new(Vec::new()),
            set_calls: Mutex::new(Vec::new()),
            subscribed: Mutex::new(Vec::new()),
            unsubscribed: Mutex::new(Vec::new()),
        }
    }

    fn get_calls_len(&self) -> usize {
        self.get_calls.lock().unwrap().len()
    }

    fn set_calls_len(&self) -> usize {
        self.set_calls.lock().unwrap().len()
    }

    fn subscribed_len(&self) -> usize {
        self.subscribed.lock().unwrap().len()
    }

    fn unsubscribed_len(&self) -> usize {
        self.unsubscribed.lock().unwrap().len()
    }

    fn take_subscribed(&self) -> Vec<Vec<String>> {
        std::mem::take(&mut *self.subscribed.lock().unwrap())
    }

    fn take_unsubscribed(&self) -> Vec<Vec<String>> {
        std::mem::take(&mut *self.unsubscribed.lock().unwrap())
    }
}

#[allow(deprecated)]
impl Listener for ParameterListener {
    fn on_get_parameters(
        &self,
        _client: &foxglove::remote_access::Client,
        param_names: Vec<String>,
        _request_id: Option<&str>,
    ) -> Vec<Parameter> {
        self.get_calls.lock().unwrap().push(param_names.clone());
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
        self.set_calls.lock().unwrap().push(parameters.clone());
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

// ---------------------------------------------------------------------------
// Mock parameter handler
// ---------------------------------------------------------------------------

/// A mock [`ParameterHandler`] that handles parameter get/set requests.
struct ParameterHandlerImpl {
    /// Parameters returned by `get`. Set by the test before sending requests.
    stored_parameters: Mutex<Vec<Parameter>>,
    /// Records parameter names from `get` calls.
    get_calls: Mutex<Vec<Vec<String>>>,
    /// Records parameters from `set` calls.
    set_calls: Mutex<Vec<Vec<Parameter>>>,
}

impl ParameterHandlerImpl {
    fn new(initial_parameters: Vec<Parameter>) -> Self {
        Self {
            stored_parameters: Mutex::new(initial_parameters),
            get_calls: Mutex::new(Vec::new()),
            set_calls: Mutex::new(Vec::new()),
        }
    }

    fn get_calls_len(&self) -> usize {
        self.get_calls.lock().unwrap().len()
    }

    fn set_calls_len(&self) -> usize {
        self.set_calls.lock().unwrap().len()
    }
}

impl ParameterHandler for ParameterHandlerImpl {
    fn get(
        &self,
        _client: AnyClient,
        names: Vec<String>,
        _request_id: Option<String>,
        responder: GetParametersResponder,
    ) {
        self.get_calls.lock().unwrap().push(names.clone());
        let params = self.stored_parameters.lock().unwrap();
        let values = if names.is_empty() {
            params.clone()
        } else {
            params
                .iter()
                .filter(|p| names.contains(&p.name))
                .cloned()
                .collect()
        };
        responder.respond(values);
    }

    fn set(
        &self,
        _client: AnyClient,
        parameters: Vec<Parameter>,
        _request_id: Option<String>,
        responder: SetParametersResponder,
    ) {
        self.set_calls.lock().unwrap().push(parameters.clone());
        let mut stored = self.stored_parameters.lock().unwrap();
        for param in &parameters {
            if let Some(existing) = stored.iter_mut().find(|p| p.name == param.name) {
                *existing = param.clone();
            } else {
                stored.push(param.clone());
            }
        }
        responder.respond(stored.clone());
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
    let handler = Arc::new(ParameterHandlerImpl::new(vec![]));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            parameter_handler: Some(handler),
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
    let handler = Arc::new(ParameterHandlerImpl::new(params));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            parameter_handler: Some(handler),
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
    let handler = Arc::new(ParameterHandlerImpl::new(vec![Parameter::string(
        "color", "red",
    )]));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            parameter_handler: Some(handler),
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
    let handler = Arc::new(ParameterHandlerImpl::new(vec![]));
    let listener = Arc::new(ParameterListener::new(vec![]));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            parameter_handler: Some(handler),
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

/// Test that `publish_parameter_values` filters by subscription: a viewer only
/// receives parameters it subscribed to.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_publish_filters_by_subscription() -> Result<()> {
    let ctx = foxglove::Context::new();
    let handler = Arc::new(ParameterHandlerImpl::new(vec![]));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            parameter_handler: Some(handler),
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
    let handler = Arc::new(ParameterHandlerImpl::new(vec![]));
    let listener = Arc::new(ParameterListener::new(vec![]));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            parameter_handler: Some(handler),
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

/// Test that when both a `ParameterHandler` and a `Listener` are registered,
/// the handler wins for get/set while the listener still receives sub/unsub.
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_handler_takes_precedence_over_listener() -> Result<()> {
    let ctx = foxglove::Context::new();
    let handler = Arc::new(ParameterHandlerImpl::new(vec![Parameter::float64(
        "foo", 1.0,
    )]));
    let listener = Arc::new(ParameterListener::new(vec![]));
    let gw = TestGateway::start_with_options(
        &ctx,
        TestGatewayOptions {
            capabilities: vec![Capability::Parameters],
            parameter_handler: Some(handler.clone()),
            listener: Some(listener.clone()),
            ..Default::default()
        },
    )
    .await?;

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

    viewer.send_subscribe_parameter_updates(&["foo"]).await?;
    viewer.send_get_parameters_with_id(&["foo"], "g1").await?;
    viewer
        .send_set_parameters_with_id(vec![Parameter::float64("foo", 3.0)], "s1")
        .await?;
    viewer.send_unsubscribe_parameter_updates(&["foo"]).await?;

    poll_until(|| {
        handler.get_calls_len() >= 1
            && handler.set_calls_len() >= 1
            && listener.subscribed_len() >= 1
            && listener.unsubscribed_len() >= 1
    })
    .await;

    // Listener get/set methods were never invoked.
    assert_eq!(listener.get_calls_len(), 0);
    assert_eq!(listener.set_calls_len(), 0);

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

// ===========================================================================
// Legacy Listener parameter callbacks
// ===========================================================================
//
// These tests cover the deprecated `Listener::on_*_parameters*` paths in
// `session.rs`. They duplicate a representative subset of the
// `ParameterHandler` tests above so each legacy branch keeps integration
// coverage until the callbacks are removed. Delete this section when the
// listener parameter callbacks are removed.

/// Listener variant of [`livekit_parameter_get_parameters`].
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_get_parameters_via_listener() -> Result<()> {
    let ctx = foxglove::Context::new();
    let listener = Arc::new(ParameterListener::new(vec![
        Parameter::string("foo", "hello"),
        Parameter::float64("bar", 42.0),
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

    let mut viewer = ViewerConnection::connect(&gw.room_name, "viewer-1").await?;
    let _server_info = viewer.expect_server_info().await?;

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

/// Listener variant of [`livekit_parameter_set_parameters`].
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_set_parameters_via_listener() -> Result<()> {
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

/// Listener variant of [`livekit_parameter_subscribe_and_publish`].
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_subscribe_and_publish_via_listener() -> Result<()> {
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

    viewer
        .send_subscribe_parameter_updates(&["speed", "mode"])
        .await?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let subscribed = listener.take_subscribed();
    assert!(
        !subscribed.is_empty(),
        "listener should have received on_parameters_subscribe"
    );

    gw.handle
        .publish_parameter_values(vec![Parameter::float64("speed", 99.0)]);

    let response = viewer.expect_parameter_values().await?;
    info!("ParameterValues after publish: {response:?}");
    assert_eq!(response.parameters.len(), 1);
    assert_eq!(response.parameters[0].name, "speed");

    viewer.close().await?;
    gw.stop().await?;
    Ok(())
}

/// Listener variant of [`livekit_parameter_unsubscribe_stops_delivery`].
#[traced_test]
#[ignore]
#[tokio::test]
#[serial(livekit)]
async fn livekit_parameter_unsubscribe_stops_delivery_via_listener() -> Result<()> {
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

    viewer.send_subscribe_parameter_updates(&["temp"]).await?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    gw.handle
        .publish_parameter_values(vec![Parameter::float64("temp", 20.0)]);
    let response = viewer.expect_parameter_values().await?;
    assert_eq!(response.parameters.len(), 1);

    viewer.send_unsubscribe_parameter_updates(&["temp"]).await?;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let unsubscribed = listener.take_unsubscribed();
    assert!(
        !unsubscribed.is_empty(),
        "listener should have received on_parameters_unsubscribe"
    );

    gw.handle
        .publish_parameter_values(vec![Parameter::float64("temp", 30.0)]);

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

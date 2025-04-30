use assert_matches::assert_matches;
use bytes::{BufMut, Bytes, BytesMut};
use futures_util::FutureExt;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::{self, http::HeaderValue, Message};
use tracing_test::traced_test;
use tungstenite::client::IntoClientRequest;

use super::testutil::WebSocketClient;
use super::ws_protocol::client::subscribe::Subscription;
use super::ws_protocol::client::{
    self, Advertise, FetchAsset, GetParameters, ServiceCallRequest, SetParameters, Subscribe,
    SubscribeConnectionGraph, SubscribeParameterUpdates, Unsubscribe, UnsubscribeConnectionGraph,
    UnsubscribeParameterUpdates,
};
use super::ws_protocol::server::connection_graph_update::{
    AdvertisedService, PublishedTopic, SubscribedTopic,
};
use super::ws_protocol::server::{
    advertise_services, ConnectionGraphUpdate, FetchAssetResponse, ParameterValues, ServerInfo,
    ServerMessage, ServiceCallFailure, ServiceCallResponse, Status,
};
use crate::testutil::{assert_eventually, RecordingServerListener};
use crate::websocket::handshake::SUBPROTOCOL;
use crate::websocket::server::{create_server, ServerOptions};
use crate::websocket::service::{CallId, Service, ServiceSchema};
use crate::websocket::{
    BlockingAssetHandlerFn, Capability, ClientChannelId, ConnectionGraph, Parameter,
};
use crate::{ChannelBuilder, Context, FoxgloveError, PartialMetadata, RawChannel, Schema};

macro_rules! expect_recv {
    ($client:expr, $variant:path) => {{
        let msg = $client.recv().await.expect("Failed to recv");
        match msg {
            $variant(m) => m,
            _ => panic!("Received unexpected message: {msg:?}"),
        }
    }};
}

macro_rules! expect_recv_close {
    ($client:expr) => {{
        let msg = $client.recv_msg().await.expect("Failed to recv");
        match msg {
            Message::Close(_) => (),
            m => panic!("Received unexpected message: {m:?}"),
        }
    }};
}

fn new_channel(topic: &str, ctx: &Arc<Context>) -> Arc<RawChannel> {
    ChannelBuilder::new(topic)
        .message_encoding("message_encoding")
        .schema(Schema::new(
            "schema_name",
            "schema_encoding",
            b"schema_data",
        ))
        .metadata(maplit::btreemap! {"key".to_string() => "value".to_string()})
        .context(ctx)
        .build_raw()
        .expect("Failed to create channel")
}

#[traced_test]
#[tokio::test]
async fn test_client_connect() {
    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            session_id: Some("mock_sess_id".to_string()),
            name: Some("mock_server".to_string()),
            ..Default::default()
        },
    );
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;

    let msg = expect_recv!(client, ServerMessage::ServerInfo);
    assert_eq!(
        msg,
        ServerInfo::new("mock_server").with_session_id("mock_sess_id")
    );

    server.stop();
}

#[traced_test]
#[tokio::test]
async fn test_handshake_with_unknown_subprotocol_fails_on_client() {
    let ctx = Context::new();
    let server = create_server(&ctx, ServerOptions::default());
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut request = format!("ws://{addr}/")
        .into_client_request()
        .expect("Failed to build request");

    request.headers_mut().insert(
        "sec-websocket-protocol",
        HeaderValue::from_static("unknown"),
    );

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "HTTP error: 400 Bad Request"
    );
    assert!(logs_contain("Dropping client"));
}

#[traced_test]
#[tokio::test]
async fn test_handshake_with_no_subprotocol_fails_upgrade() {
    let ctx = Context::new();
    let server = create_server(&ctx, ServerOptions::default());
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut request = format!("ws://{addr}/")
        .into_client_request()
        .expect("Failed to build request");

    request
        .headers_mut()
        .insert("some-other-header", HeaderValue::from_static("1"));

    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "HTTP error: 400 Bad Request"
    );
    assert!(logs_contain("Dropping client"));
}

#[traced_test]
#[tokio::test]
async fn test_handshake_with_multiple_subprotocols() {
    let ctx = Context::new();
    let server = create_server(&ctx, ServerOptions::default());
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let request = format!("ws://{addr}/")
        .into_client_request()
        .expect("Failed to build request");

    let mut req1 = request.clone();
    let header = format!("{}, foxglove.sdk.v2", SUBPROTOCOL);
    req1.headers_mut().insert(
        "sec-websocket-protocol",
        HeaderValue::from_str(&header).unwrap(),
    );

    let (_, response) = tokio_tungstenite::connect_async(req1)
        .await
        .expect("Failed to connect");

    assert_eq!(
        response.headers().get("sec-websocket-protocol"),
        Some(&HeaderValue::from_static(SUBPROTOCOL))
    );

    // In req2, the client's preferred (initial) subprotocol is not valid
    let mut req2 = request.clone();
    let header = format!("unknown, {}, another", SUBPROTOCOL);
    req2.headers_mut().insert(
        "sec-websocket-protocol",
        HeaderValue::from_str(&header).unwrap(),
    );

    let (_, response) = tokio_tungstenite::connect_async(req2)
        .await
        .expect("Failed to connect");

    assert_eq!(
        response.headers().get("sec-websocket-protocol"),
        Some(&HeaderValue::from_static(SUBPROTOCOL))
    );

    server.stop();
}

#[traced_test]
#[tokio::test]
async fn test_advertise_to_client() {
    let recording_listener = Arc::new(RecordingServerListener::new());

    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            listener: Some(recording_listener.clone()),
            ..Default::default()
        },
    );

    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;
    expect_recv!(client, ServerMessage::ServerInfo);

    let ch = new_channel("/foo", &ctx);
    ch.log(b"foo bar");

    let msg = expect_recv!(client, ServerMessage::Advertise);
    let adv_ch = msg.channels.first().expect("not empty");
    assert_eq!(adv_ch.id, u64::from(ch.id()));
    assert_eq!(adv_ch.topic, ch.topic());

    let subscription_id = 42;
    let subscribe_msg = Subscribe::new([Subscription {
        id: subscription_id,
        channel_id: ch.id().into(),
    }]);
    client.send(&subscribe_msg).await.expect("Failed to send");

    // Allow the server to process the subscription
    assert_eventually(|| dbg!(ch.num_sinks()) == 1).await;

    ch.log(b"{\"a\":1}");

    let msg = expect_recv!(client, ServerMessage::MessageData);
    assert_eq!(msg.subscription_id, subscription_id);

    let subscriptions = recording_listener.take_subscribe();
    assert_eq!(subscriptions.len(), 1);
    assert_eq!(subscriptions[0].1.id, ch.id());
    assert_eq!(subscriptions[0].1.topic, ch.topic());

    // Send a duplicate subscribe message, get an error.
    client.send(&subscribe_msg).await.expect("Failed to send");
    let msg = expect_recv!(client, ServerMessage::Status);
    assert_eq!(
        msg,
        Status::warning(format!(
            "Client is already subscribed to channel: {}; ignoring subscription",
            ch.id(),
        ))
    );

    server.stop();
}

#[traced_test]
#[tokio::test]
async fn test_advertise_schemaless_channels() {
    let recording_listener = Arc::new(RecordingServerListener::new());

    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            listener: Some(recording_listener.clone()),
            ..Default::default()
        },
    );

    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;
    expect_recv!(client, ServerMessage::ServerInfo);

    // Client receives the correct advertisement for schemaless JSON
    let json_chan = ChannelBuilder::new("/schemaless_json")
        .message_encoding("json")
        .context(&ctx)
        .build_raw()
        .expect("Failed to create channel");

    json_chan.log(b"{\"a\": 1}");

    let msg = expect_recv!(client, ServerMessage::Advertise);
    let adv_chan = msg.channels.first().expect("not empty");
    assert_eq!(adv_chan.id, u64::from(json_chan.id()));
    assert_eq!(adv_chan.topic, json_chan.topic());

    // Client receives no advertisements for other schemaless channels (not supported)
    let invalid_chan = ChannelBuilder::new("/schemaless_other")
        .message_encoding("protobuf")
        .context(&ctx)
        .build_raw()
        .expect("Failed to create channel");

    invalid_chan.log(b"1");

    assert!(client.recv().now_or_never().is_none());

    assert!(logs_contain(
        "Ignoring advertise channel for /schemaless_other because a schema is required"
    ));

    server.stop();
}

#[traced_test]
#[tokio::test]
async fn test_log_only_to_subscribers() {
    let recording_listener = Arc::new(RecordingServerListener::new());

    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            listener: Some(recording_listener.clone()),
            ..Default::default()
        },
    );

    let ch1 = new_channel("/foo", &ctx);
    let ch2 = new_channel("/bar", &ctx);
    let ch3 = new_channel("/baz", &ctx);

    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client1 = WebSocketClient::connect(addr).await;
    let mut client2 = WebSocketClient::connect(addr).await;
    let mut client3 = WebSocketClient::connect(addr).await;

    // client1 subscribes to ch1; client2 subscribes to ch2; client3 unsubscribes from all
    // Read the server info message from each
    expect_recv!(client1, ServerMessage::ServerInfo);
    expect_recv!(client2, ServerMessage::ServerInfo);
    expect_recv!(client3, ServerMessage::ServerInfo);

    // Read the channel advertisement from each client for all 3 channels
    let expect_ch_ids: Vec<_> = [&ch1, &ch2, &ch3]
        .iter()
        .map(|c| u64::from(c.id()))
        .collect();
    for client in [&mut client1, &mut client2, &mut client3] {
        let msg = expect_recv!(client, ServerMessage::Advertise);
        let mut ch_ids: Vec<_> = msg.channels.iter().map(|c| c.id).collect();
        ch_ids.sort_unstable();
        assert_eq!(&ch_ids, &expect_ch_ids);
    }

    client1
        .send(&Subscribe::new([Subscription {
            id: 1,
            channel_id: ch1.id().into(),
        }]))
        .await
        .expect("Failed to send");

    client2
        .send(&Subscribe::new([Subscription {
            id: 2,
            channel_id: ch2.id().into(),
        }]))
        .await
        .expect("Failed to send");

    // Allow the server to process the subscriptions
    assert_eventually(|| dbg!(ch1.num_sinks()) == 1 && dbg!(ch2.num_sinks()) == 1).await;

    client3
        .send(&Subscribe::new([
            Subscription {
                id: 111,
                channel_id: ch1.id().into(),
            },
            Subscription {
                id: 222,
                channel_id: ch2.id().into(),
            },
        ]))
        .await
        .expect("Failed to send");

    // Allow the server to process the subscriptions
    assert_eventually(|| dbg!(ch1.num_sinks()) == 2 && dbg!(ch2.num_sinks()) == 2).await;

    client3
        .send(&Unsubscribe::new([111, 222]))
        .await
        .expect("Failed to send");

    // Allow the server to process the unsubscriptions
    assert_eventually(|| dbg!(ch1.num_sinks()) == 1 && dbg!(ch2.num_sinks()) == 1).await;

    let subscriptions = recording_listener.take_subscribe();
    assert_eq!(subscriptions.len(), 4);
    assert_eq!(subscriptions[0].1.id, ch1.id());
    assert_eq!(subscriptions[1].1.id, ch2.id());
    assert_eq!(subscriptions[2].1.id, ch1.id());
    assert_eq!(subscriptions[3].1.id, ch2.id());
    assert_eq!(subscriptions[0].1.topic, ch1.topic());
    assert_eq!(subscriptions[1].1.topic, ch2.topic());
    assert_eq!(subscriptions[2].1.topic, ch1.topic());
    assert_eq!(subscriptions[3].1.topic, ch2.topic());

    let unsubscriptions = recording_listener.take_unsubscribe();
    assert_eq!(unsubscriptions.len(), 2);
    assert_eq!(unsubscriptions[0].1.id, ch1.id());
    assert_eq!(unsubscriptions[1].1.id, ch2.id());
    assert_eq!(unsubscriptions[0].1.topic, ch1.topic());
    assert_eq!(unsubscriptions[1].1.topic, ch2.topic());

    let metadata = PartialMetadata {
        log_time: Some(123456),
    };
    ch3.log_with_meta(b"channel3", metadata);
    ch2.log_with_meta(b"channel2", metadata);
    ch1.log_with_meta(b"channel1", metadata);

    // Receive the message for client1 and client2
    let msg = expect_recv!(client1, ServerMessage::MessageData);
    assert_eq!(msg.subscription_id, 1);
    assert_eq!(msg.log_time, 123456);
    assert_eq!(msg.data, Cow::Borrowed(b"channel1"));

    let msg = expect_recv!(client2, ServerMessage::MessageData);
    assert_eq!(msg.subscription_id, 2);
    assert_eq!(msg.log_time, 123456);
    assert_eq!(msg.data, Cow::Borrowed(b"channel2"));

    // Client 3 should not receive any messages since it unsubscribed from all channels
    assert!(client3.recv().now_or_never().is_none());

    server.stop();
}

#[tokio::test]
async fn test_on_unsubscribe_called_after_disconnect() {
    let recording_listener = Arc::new(RecordingServerListener::new());

    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            listener: Some(recording_listener.clone()),
            ..Default::default()
        },
    );

    let chan = new_channel("/foo", &ctx);
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;
    expect_recv!(client, ServerMessage::ServerInfo);
    expect_recv!(client, ServerMessage::Advertise);

    client
        .send(&Subscribe::new([Subscription {
            id: 1,
            channel_id: chan.id().into(),
        }]))
        .await
        .expect("Failed to send");

    // Allow the server to process the subscriptions
    assert_eventually(|| dbg!(chan.num_sinks()) == 1).await;

    let subscriptions = recording_listener.take_subscribe();
    assert_eq!(subscriptions.len(), 1);

    let unsubscriptions = recording_listener.take_unsubscribe();
    assert_eq!(unsubscriptions.len(), 0);

    // Disconnect the client without unsubscribing explicitly
    client.close().await;

    // Allow the server to process the disconnection
    assert_eventually(|| dbg!(chan.num_sinks()) == 0).await;

    let unsubscriptions = recording_listener.take_unsubscribe();
    assert_eq!(unsubscriptions.len(), 1);

    server.stop();
}

#[traced_test]
#[tokio::test]
async fn test_error_when_client_publish_unsupported() {
    // Server does not support clientPublish capability by default
    let ctx = Context::new();
    let server = create_server(&ctx, ServerOptions::default());
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;
    expect_recv!(client, ServerMessage::ServerInfo);

    let advertise = Advertise::new([client::advertise::Channel::builder(1, "/test", "json")
        .build()
        .unwrap()]);
    client
        .send(&advertise)
        .await
        .expect("Failed to send advertisement");

    // Server should respond with an error status
    let msg = expect_recv!(client, ServerMessage::Status);
    assert_eq!(
        msg,
        Status::error("Server does not support clientPublish capability")
    );

    client.close().await;
    server.stop();
}

#[traced_test]
#[tokio::test]
async fn test_error_status_message() {
    let ctx = Context::new();
    let server = create_server(&ctx, ServerOptions::default());
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;
    expect_recv!(client, ServerMessage::ServerInfo);

    {
        client
            .send(Message::text("nonsense".to_string()))
            .await
            .expect("Failed to send message");

        assert_eq!(
            expect_recv!(client, ServerMessage::Status),
            Status::error("Invalid message: expected ident at line 1 column 2")
        );
    }

    {
        client
            .send(&Subscribe::new([Subscription {
                id: 1,
                channel_id: 555,
            }]))
            .await
            .expect("Failed to send message");

        assert_eq!(
            expect_recv!(client, ServerMessage::Status),
            Status::error("Unknown channel ID: 555")
        );
    }

    {
        client
            .send(Message::binary(vec![0xff]))
            .await
            .expect("Failed to send message");

        assert_eq!(
            expect_recv!(client, ServerMessage::Status),
            Status::error("Invalid message: Unknown binary opcode 255")
        );
    }

    server.stop();
}

fn svc_unreachable(_: super::service::Request) -> Result<Bytes, String> {
    unreachable!("this service handler is never invoked")
}

#[tokio::test]
async fn test_service_registration_not_supported() {
    // Can't register services if we don't declare support.
    let ctx = Context::new();
    let server = create_server(&ctx, ServerOptions::default());
    let svc = Service::builder("/s", ServiceSchema::new("")).handler_fn(svc_unreachable);
    assert_matches!(
        server.add_services(vec![svc]),
        Err(FoxgloveError::ServicesNotSupported)
    );
}

#[tokio::test]
async fn test_service_registration_missing_request_encoding() {
    // Can't register a service with no encoding unless we declare global encodings.
    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            capabilities: Some(HashSet::from([Capability::Services])),
            ..Default::default()
        },
    );
    let svc = Service::builder("/s", ServiceSchema::new("")).handler_fn(svc_unreachable);
    assert_matches!(
        server.add_services(vec![svc]),
        Err(FoxgloveError::MissingRequestEncoding(_))
    );
}

#[tokio::test]
async fn test_service_registration_duplicate_name() {
    // Can't register a service with no encoding unless we declare global encodings.
    let ctx = Context::new();
    let sa1 = Service::builder("/a", ServiceSchema::new("")).handler_fn(svc_unreachable);
    let server = create_server(
        &ctx,
        ServerOptions {
            capabilities: Some(HashSet::from([Capability::Services])),
            services: HashMap::from([(sa1.name().to_string(), sa1)]),
            supported_encodings: Some(HashSet::from(["ros1msg".into()])),
            ..Default::default()
        },
    );

    let sa2 = Service::builder("/a", ServiceSchema::new("")).handler_fn(svc_unreachable);
    assert_matches!(
        server.add_services(vec![sa2]),
        Err(FoxgloveError::DuplicateService(_))
    );

    let sb1 = Service::builder("/b", ServiceSchema::new("")).handler_fn(svc_unreachable);
    let sb2 = Service::builder("/b", ServiceSchema::new("")).handler_fn(svc_unreachable);
    assert_matches!(
        server.add_services(vec![sb1, sb2]),
        Err(FoxgloveError::DuplicateService(_))
    );
}

#[traced_test]
#[tokio::test]
async fn test_publish_status_message() {
    let ctx = Context::new();
    let server = create_server(&ctx, ServerOptions::default());

    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;
    expect_recv!(client, ServerMessage::ServerInfo);

    let statuses = [
        Status::info("Hello, world!").with_id("123"),
        Status::warning("Situation unusual"),
        Status::error("Reactor core overload!").with_id("abc"),
    ];
    for status in statuses {
        server.publish_status(status.clone());
        let msg = expect_recv!(client, ServerMessage::Status);
        assert_eq!(msg, status);
    }
}

#[traced_test]
#[tokio::test]
async fn test_remove_status() {
    let ctx = Context::new();
    let server = create_server(&ctx, ServerOptions::default());
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client1 = WebSocketClient::connect(addr).await;
    let mut client2 = WebSocketClient::connect(addr).await;
    expect_recv!(client1, ServerMessage::ServerInfo);
    expect_recv!(client2, ServerMessage::ServerInfo);

    // These don't have to exist, and aren't checked
    let ids = vec!["123".to_string(), "abc".to_string()];
    server.remove_status(ids.clone());
    for mut client in [client1, client2] {
        let msg = expect_recv!(client, ServerMessage::RemoveStatus);
        assert_eq!(msg.status_ids, ids);
    }
}

#[traced_test]
#[tokio::test]
async fn test_client_advertising() {
    let recording_listener = Arc::new(RecordingServerListener::new());

    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            capabilities: Some(HashSet::from([Capability::ClientPublish])),
            supported_encodings: Some(HashSet::from(["json".to_string()])),
            listener: Some(recording_listener.clone()),
            ..Default::default()
        },
    );

    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;
    expect_recv!(client, ServerMessage::ServerInfo);

    let channel_id = 1;
    let data = b"{\"a\":1}";
    let msg_data = client::MessageData::new(channel_id, data);

    // Send before advertising: message is dropped
    client
        .send(&msg_data)
        .await
        .expect("Failed to send binary message");

    // No message sent to listener
    assert!(recording_listener.take_message_data().is_empty());

    let advertise =
        client::Advertise::new([
            client::advertise::Channel::builder(channel_id, "/test", "json")
                .build()
                .unwrap(),
        ]);
    client
        .send(&advertise)
        .await
        .expect("Failed to send advertisement");

    // Send duplicate advertisement: no effect
    client
        .send(&advertise)
        .await
        .expect("Failed to send advertisement");

    // Send message after advertising
    client
        .send(&msg_data)
        .await
        .expect("Failed to send binary message");

    // Does not error on a binary message with no opcode
    client
        .send(Message::binary(vec![]))
        .await
        .expect("Failed to send empty binary message");

    let unadvertise = client::Unadvertise::new([channel_id]);
    client
        .send(&unadvertise)
        .await
        .expect("Failed to send unadvertise");

    // Should be ignored
    client
        .send(&unadvertise)
        .await
        .expect("Failed to send unadvertise");

    assert_eventually(|| {
        dbg!(recording_listener.message_data_len()) == 1
            && dbg!(recording_listener.client_advertise_len()) == 1
            && dbg!(recording_listener.client_unadvertise_len()) == 1
    })
    .await;

    // Server should have received one message
    let mut received = recording_listener.take_message_data();
    let message_data = received.pop().expect("No message received");
    assert_eq!(message_data.channel.id, ClientChannelId::new(1));
    assert_eq!(message_data.data, data);

    // Server should have ignored the duplicate advertisement
    let advertisements = recording_listener.take_client_advertise();
    assert_eq!(advertisements.len(), 1);
    assert_eq!(advertisements[0].1.id, ClientChannelId::new(channel_id));

    // Server should have received one unadvertise (and ignored the duplicate)
    let unadvertises = recording_listener.take_client_unadvertise();
    assert_eq!(unadvertises.len(), 1);
    assert_eq!(unadvertises[0].1.id, ClientChannelId::new(channel_id));

    client.close().await;
    server.stop();
}

#[traced_test]
#[tokio::test]
async fn test_parameter_values() {
    let ctx = Context::new();
    let recording_listener = Arc::new(RecordingServerListener::new());
    let server = create_server(
        &ctx,
        ServerOptions {
            capabilities: Some(HashSet::from([Capability::Parameters])),
            listener: Some(recording_listener.clone()),
            ..Default::default()
        },
    );
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;

    // Send the Subscribe Parameter Update message for "some-float-value"
    // Otherwise we won't get the update after we publish it.
    client
        .send(&SubscribeParameterUpdates::new(["some-float-value"]))
        .await
        .expect("Failed to send subscribe parameter updates");

    expect_recv!(client, ServerMessage::ServerInfo);

    assert_eventually(|| dbg!(recording_listener.parameters_subscribe_len()) == 1).await;

    let parameter = Parameter::float64("some-float-value", 1.23);
    server.publish_parameter_values(vec![parameter.clone()]);

    let msg = expect_recv!(client, ServerMessage::ParameterValues);
    assert_eq!(msg, ParameterValues::new([parameter]));

    server.stop();
}

#[traced_test]
#[tokio::test]
async fn test_parameter_unsubscribe_no_updates() {
    let recording_listener = Arc::new(RecordingServerListener::new());

    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            capabilities: Some(HashSet::from([Capability::Parameters])),
            listener: Some(recording_listener.clone()),
            ..Default::default()
        },
    );
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;

    // Send the Subscribe Parameter Update message for "some-float-value"
    client
        .send(&SubscribeParameterUpdates::new(["some-float-value"]))
        .await
        .expect("Failed to send subscribe parameter updates");

    // Send the Unsubscribe Parameter Update message for "some-float-value"
    client
        .send(&UnsubscribeParameterUpdates::new([
            "some-float-value",
            "baz",
        ]))
        .await
        .expect("Failed to send unsubscribe parameter updates");

    expect_recv!(client, ServerMessage::ServerInfo);

    assert_eventually(|| {
        dbg!(recording_listener.parameters_subscribe_len()) == 1
            && dbg!(recording_listener.parameters_unsubscribe_len()) == 1
    })
    .await;

    let parameter_names = recording_listener
        .take_parameters_subscribe()
        .pop()
        .unwrap();
    assert_eq!(parameter_names, vec!["some-float-value"]);

    let parameter_names = recording_listener
        .take_parameters_unsubscribe()
        .pop()
        .unwrap();
    assert_eq!(parameter_names, vec!["some-float-value"]);

    let parameter = Parameter::float64("some-float-value", 1.23);
    server.publish_parameter_values(vec![parameter]);

    // Sleep for a little while to give the server time to flush its queues, to ensure that it
    // doesn't send a parameter message to an unsubscribed client.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    server.stop();

    // No parameter message was sent with the updated param before the Close message
    expect_recv_close!(client);
}

#[traced_test]
#[tokio::test]
async fn test_set_parameters() {
    let recording_listener = Arc::new(RecordingServerListener::new());

    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            capabilities: Some(HashSet::from([Capability::Parameters])),
            listener: Some(recording_listener.clone()),
            ..Default::default()
        },
    );
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;

    // Subscribe to "foo" and "bar"
    client
        .send(&SubscribeParameterUpdates::new(["foo", "bar"]))
        .await
        .expect("Failed to send subscribe parameter updates");

    assert_eventually(|| dbg!(recording_listener.parameters_subscribe_len()) == 1).await;

    let parameters = vec![
        Parameter::float64("foo", 1.0),
        Parameter::string("bar", "hello"),
        Parameter::bool("baz", true),
    ];
    client
        .send(&SetParameters::new(parameters.clone()).with_id("123"))
        .await
        .expect("Failed to send set parameters");

    expect_recv!(client, ServerMessage::ServerInfo);

    // setParameters returns the result of on_set_parameters, which for recording listener, just returns them back
    let msg = expect_recv!(client, ServerMessage::ParameterValues);
    assert_eq!(msg, ParameterValues::new(parameters.clone()).with_id("123"));

    // it will also publish the updated paramters returned from on_set_parameters
    // which will send just the paramters we're subscribed to.
    let msg = expect_recv!(client, ServerMessage::ParameterValues);
    assert_eq!(msg, ParameterValues::new(parameters[..2].to_vec()));

    let set_parameters = recording_listener.take_parameters_set().pop().unwrap();
    assert_eq!(set_parameters.request_id, Some("123".to_string()));
    assert_eq!(set_parameters.parameters, parameters);

    server.stop();
}

#[traced_test]
#[tokio::test]
async fn test_get_parameters() {
    let parameters = vec![Parameter::float64("foo", 1.0)];
    let recording_listener = Arc::new(RecordingServerListener::new());
    recording_listener.set_parameters_get_result(parameters.clone());

    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            capabilities: Some(HashSet::from([Capability::Parameters])),
            listener: Some(recording_listener.clone()),
            ..Default::default()
        },
    );
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;

    client
        .send(&GetParameters::new(["foo", "bar", "baz"]).with_id("123"))
        .await
        .expect("Failed to send get parameters");

    expect_recv!(client, ServerMessage::ServerInfo);
    let msg = expect_recv!(client, ServerMessage::ParameterValues);
    assert_eq!(msg.id, Some("123".into()));
    assert_eq!(msg.parameters, parameters);

    let get_parameters = recording_listener.take_parameters_get().pop().unwrap();
    assert_eq!(get_parameters.param_names, vec!["foo", "bar", "baz"]);
    assert_eq!(get_parameters.request_id, Some("123".to_string()));

    server.stop();
}

#[tokio::test]
async fn test_services() {
    let ok_svc = Service::builder("/ok", ServiceSchema::new("plain")).handler_fn(
        |req| -> Result<Bytes, String> {
            assert_eq!(req.service_name(), "/ok");
            assert_eq!(req.call_id(), CallId::new(99));
            let payload = req.into_payload();
            let mut response = BytesMut::with_capacity(payload.len());
            response.put(payload);
            response.reverse();
            Ok(response.freeze())
        },
    );
    let ok_svc_id = u32::from(ok_svc.id());

    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            services: [ok_svc]
                .into_iter()
                .map(|s| (s.name().to_string(), s))
                .collect(),
            supported_encodings: Some(HashSet::from(["raw".to_string()])),
            ..Default::default()
        },
    );

    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client1 = WebSocketClient::connect(addr).await;
    expect_recv!(client1, ServerMessage::ServerInfo);
    let msg = expect_recv!(client1, ServerMessage::AdvertiseServices);
    assert_eq!(
        msg.services,
        vec![advertise_services::Service::new(ok_svc_id, "/ok", "plain")]
    );

    // Send a request.
    let ok_req = ServiceCallRequest {
        service_id: ok_svc_id,
        call_id: 99,
        encoding: "raw".into(),
        payload: b"payload".into(),
    };
    client1.send(&ok_req).await.expect("Failed to send");

    // Validate the response.
    let msg = expect_recv!(client1, ServerMessage::ServiceCallResponse);
    assert_eq!(
        msg,
        ServiceCallResponse {
            service_id: ok_svc_id,
            call_id: 99,
            encoding: "raw".into(),
            payload: b"daolyap".into(),
        }
    );

    // Register a new service.
    let err_svc = Service::builder("/err", ServiceSchema::new("plain"))
        .handler_fn(|_| Err::<&[u8], _>("oh noes"));
    let err_svc_id = u32::from(err_svc.id());
    server
        .add_services(vec![err_svc])
        .expect("Failed to add service");

    let msg = expect_recv!(client1, ServerMessage::AdvertiseServices);
    assert_eq!(
        msg.services,
        vec![advertise_services::Service::new(
            err_svc_id, "/err", "plain"
        )]
    );

    // Send a request to the error service.
    client1
        .send(&ServiceCallRequest {
            service_id: err_svc_id,
            call_id: 11,
            encoding: "raw".into(),
            payload: b"payload".into(),
        })
        .await
        .expect("Failed to send");

    // Validate the error response.
    let msg = expect_recv!(client1, ServerMessage::ServiceCallFailure);
    assert_eq!(
        msg,
        ServiceCallFailure {
            service_id: err_svc_id,
            call_id: 11,
            message: "oh noes".into(),
        }
    );

    // New client sees both services immediately.
    let mut client2 = WebSocketClient::connect(addr).await;
    expect_recv!(client2, ServerMessage::ServerInfo);
    let msg = expect_recv!(client2, ServerMessage::AdvertiseServices);
    assert_eq!(msg.services.len(), 2);
    drop(client2);

    // Unregister services.
    server.remove_services(["/ok"]);
    let msg = expect_recv!(client1, ServerMessage::UnadvertiseServices);
    assert_eq!(msg.service_ids, vec![ok_svc_id]);

    // Try to call the now-unregistered service.
    client1.send(&ok_req).await.expect("Failed to send");

    // Validate the error response.
    let msg = expect_recv!(client1, ServerMessage::ServiceCallFailure);
    assert_eq!(
        msg,
        ServiceCallFailure {
            service_id: ok_svc_id,
            call_id: 99,
            message: "Unknown service".into(),
        }
    );

    // Add a service that always panics.
    let panic_svc = Service::builder("/panic", ServiceSchema::new("raw"))
        .blocking_handler_fn(|_| -> Result<Bytes, String> { panic!("oh noes") });
    let panic_svc_id = u32::from(panic_svc.id());
    server
        .add_services(vec![panic_svc])
        .expect("Failed to add service");

    expect_recv!(client1, ServerMessage::AdvertiseServices);

    // Send a request to the panic service.
    client1
        .send(&ServiceCallRequest {
            service_id: panic_svc_id,
            call_id: 22,
            encoding: "raw".into(),
            payload: b"payload".into(),
        })
        .await
        .expect("Failed to send");

    // Validate the error response.
    let msg = expect_recv!(client1, ServerMessage::ServiceCallFailure);
    assert_eq!(
        msg,
        ServiceCallFailure {
            service_id: panic_svc_id,
            call_id: 22,
            message: "Internal server error: service failed to send a response".into(),
        }
    );
}

#[tokio::test]
async fn test_fetch_asset() {
    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            capabilities: Some(HashSet::from([Capability::Assets])),
            fetch_asset_handler: Some(Box::new(BlockingAssetHandlerFn(Arc::new(
                |_client, uri: String| {
                    if uri.ends_with("error") {
                        Err("test error")
                    } else if uri.ends_with("panic") {
                        panic!("oh no")
                    } else {
                        Ok(b"test data")
                    }
                },
            )))),
            ..Default::default()
        },
    );
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;
    expect_recv!(client, ServerMessage::ServerInfo);

    #[derive(Debug)]
    struct Case<'a> {
        uri: &'a str,
        expect: Result<&'a [u8], &'a str>,
    }
    impl<'a> Case<'a> {
        fn new(uri: &'a str, expect: Result<&'a [u8], &'a str>) -> Self {
            Self { uri, expect }
        }
    }
    let cases = [
        Case::new("https://example.com/asset.png", Ok(b"test data")),
        Case::new(
            "https://example.com/panic",
            Err("Internal server error: asset handler failed to send a response"),
        ),
        Case::new("https://example.com/error", Err("test error")),
    ];
    for (request_id, case) in cases.iter().enumerate() {
        dbg!(case);
        let request_id = request_id as u32;
        client
            .send(&FetchAsset::new(request_id, case.uri))
            .await
            .unwrap();

        let msg = expect_recv!(client, ServerMessage::FetchAssetResponse);
        match case.expect {
            Ok(data) => assert_eq!(msg, FetchAssetResponse::asset_data(request_id, data)),
            Err(err) => assert_eq!(msg, FetchAssetResponse::error_message(request_id, err)),
        }
    }
}

#[traced_test]
#[tokio::test]
async fn test_update_connection_graph() {
    let recording_listener = Arc::new(RecordingServerListener::new());

    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            capabilities: Some(HashSet::from([Capability::ConnectionGraph])),
            listener: Some(recording_listener.clone()),
            ..Default::default()
        },
    );
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut initial_graph = ConnectionGraph::new();
    initial_graph.set_published_topic("topic1", ["publisher1".to_string()]);
    initial_graph.set_subscribed_topic("topic1", ["subscriber1".to_string()]);
    initial_graph.set_advertised_service("service1", ["provider1".to_string()]);
    server
        .replace_connection_graph(initial_graph)
        .expect("failed to update connection graph");

    assert_eq!(recording_listener.take_connection_graph_subscribe(), 0);
    assert_eq!(recording_listener.take_connection_graph_unsubscribe(), 0);

    // Connect a client and subscribe to graph updates.
    let mut c1 = WebSocketClient::connect(addr).await;
    c1.send(&SubscribeConnectionGraph {}).await.unwrap();

    // There should be a server listener callback, since this is the first subscriber.
    assert_eventually(|| dbg!(recording_listener.take_connection_graph_subscribe() == 1)).await;

    // First client gets the complete initial state upon subscribing.
    expect_recv!(c1, ServerMessage::ServerInfo);
    let msg = expect_recv!(c1, ServerMessage::ConnectionGraphUpdate);
    assert_eq!(
        msg,
        ConnectionGraphUpdate {
            published_topics: vec![PublishedTopic::new("topic1", ["publisher1"]),],
            subscribed_topics: vec![SubscribedTopic::new("topic1", ["subscriber1"]),],
            advertised_services: vec![AdvertisedService::new("service1", ["provider1"]),],
            removed_topics: vec![],
            removed_services: vec![],
        }
    );

    // Connect a second client and subscribe to graph updates.
    let mut c2 = WebSocketClient::connect(addr).await;
    c2.send(&SubscribeConnectionGraph {}).await.unwrap();

    // Second client also gets the complete initial state.
    expect_recv!(c2, ServerMessage::ServerInfo);
    let msg = expect_recv!(c2, ServerMessage::ConnectionGraphUpdate);
    assert_eq!(
        msg,
        ConnectionGraphUpdate {
            published_topics: vec![PublishedTopic::new("topic1", ["publisher1"]),],
            subscribed_topics: vec![SubscribedTopic::new("topic1", ["subscriber1"]),],
            advertised_services: vec![AdvertisedService::new("service1", ["provider1"]),],
            removed_topics: vec![],
            removed_services: vec![],
        }
    );

    // There was no listener callback for subscribe, because c1 is already an active subscriber.
    assert_eq!(recording_listener.take_connection_graph_subscribe(), 0);

    // Close the client. No unsubscribe callback because c1 is still subscribed. Since the server
    // processes the client disconnect asynchronously, just sleep a little while.
    c2.close().await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(recording_listener.take_connection_graph_unsubscribe(), 0);

    let mut graph = ConnectionGraph::new();
    // Update publisher for topic1
    graph.set_published_topic("topic1", ["publisher2".to_string()]);
    // Add topic2, remove topic1
    graph.set_subscribed_topic("topic2", ["subscriber2".to_string()]);
    // Delete service1
    server
        .replace_connection_graph(graph)
        .expect("failed to update connection graph");

    let msg = expect_recv!(c1, ServerMessage::ConnectionGraphUpdate);
    assert_eq!(
        msg,
        ConnectionGraphUpdate {
            published_topics: vec![PublishedTopic::new("topic1", ["publisher2"]),],
            subscribed_topics: vec![SubscribedTopic::new("topic2", ["subscriber2"]),],
            advertised_services: vec![],
            removed_topics: vec![],
            removed_services: vec!["service1".to_string()],
        }
    );

    // Last client unsubscribes, expect a listener callback for that.
    assert_eq!(recording_listener.take_connection_graph_unsubscribe(), 0);
    c1.send(&UnsubscribeConnectionGraph {}).await.unwrap();
    assert_eventually(|| dbg!(recording_listener.take_connection_graph_unsubscribe() == 1)).await;

    server.stop();
}

#[traced_test]
#[tokio::test]
async fn test_slow_client() {
    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            message_backlog_size: Some(1),
            ..Default::default()
        },
    );
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;

    // Publish more status messages than the client can handle
    for i in 0..50 {
        let status = Status::error(format!("msg{}", i));
        server.publish_status(status);
    }

    expect_recv!(client, ServerMessage::ServerInfo);

    for _ in 0..51 {
        // Client should have been disconnected
        let msg = expect_recv!(client, ServerMessage::Status);
        if msg.message.starts_with("msg") {
            continue;
        }
        assert_eq!(
            msg.message,
            "Disconnected because the message backlog on the server is full. The backlog size is configurable in the server setup."
        );
        break;
    }

    // Close message should be received
    expect_recv_close!(client);
    server.stop();
}

#[cfg(feature = "unstable")]
#[tokio::test]
async fn test_broadcast_time() {
    let ctx = Context::new();
    let server = create_server(
        &ctx,
        ServerOptions {
            capabilities: Some(HashSet::from([Capability::Time])),
            ..Default::default()
        },
    );
    let addr = server
        .start("127.0.0.1", 0)
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(addr).await;
    expect_recv!(client, ServerMessage::ServerInfo);

    server.broadcast_time(42);
    let msg = expect_recv!(client, ServerMessage::Time);
    assert_eq!(msg.timestamp, 42);
}

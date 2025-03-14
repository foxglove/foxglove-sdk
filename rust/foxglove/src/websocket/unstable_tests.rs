use std::collections::HashSet;

use crate::Context;

use super::tests::connect_client;
use super::{create_server, protocol, Capability, ServerOptions};
use bytes::Buf;
use futures_util::StreamExt;
use tokio_tungstenite::tungstenite::Message;

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

    let mut ws_client = connect_client(addr).await;
    _ = ws_client.next().await.expect("serverInfo");

    server.broadcast_time(42);
    let msg = ws_client
        .next()
        .await
        .expect("no message received")
        .expect("failed to parse message");
    let Message::Binary(mut buf) = msg else {
        panic!("unexpected message type");
    };
    assert_eq!(buf.get_u8(), protocol::server::BinaryOpcode::TimeData as u8);
    assert_eq!(buf.get_u64_le(), 42);
}

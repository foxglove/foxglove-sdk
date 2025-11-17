#[cfg(feature = "live_visualization")]
use std::io::BufWriter;
#[cfg(feature = "live_visualization")]
use tempfile::NamedTempFile;

#[cfg(feature = "live_visualization")]
use crate::{
    schemas::Log,
    testutil::{assert_eventually, read_summary},
    websocket::ws_protocol::{
        client::{subscribe::Subscription, Subscribe},
        server::ServerMessage,
    },
    Channel, ChannelBuilder, Context, McapWriter, WebSocketServer,
};

#[cfg(feature = "live_visualization")]
macro_rules! expect_recv {
    ($client:expr, $variant:path) => {{
        let msg = $client.recv().await.expect("Failed to recv");
        match msg {
            $variant(m) => m,
            _ => panic!("Received unexpected message: {msg:?}"),
        }
    }};
}

#[cfg(feature = "live_visualization")]
#[tokio::test(flavor = "multi_thread")]
async fn test_sink_channel_filtering_on_mcap_and_ws() {
    use crate::WebSocketClient;

    let ctx = Context::new();

    // MCAP only sees topic /1
    let file = NamedTempFile::new().unwrap();
    let mcap = McapWriter::new()
        .context(&ctx)
        .channel_filter_fn(|channel| channel.topic() == "/1")
        .create(BufWriter::new(file))
        .unwrap();

    // WS only sees topic /2
    let _ = WebSocketServer::new()
        .bind("127.0.0.1", 11011)
        .context(&ctx)
        .channel_filter_fn(|channel| channel.topic() == "/2")
        .start()
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect("127.0.0.1:11011")
        .await
        .expect("failed to connect");
    expect_recv!(client, ServerMessage::ServerInfo);

    let ch1: Channel<Log> = ChannelBuilder::new("/1").context(&ctx).build();
    let ch2: Channel<Log> = ChannelBuilder::new("/2").context(&ctx).build();

    expect_recv!(client, ServerMessage::Advertise);
    let subscription_id = 999;
    let subscribe_msg = Subscribe::new([Subscription {
        id: subscription_id,
        channel_id: ch2.id().into(),
    }]);
    client.send(&subscribe_msg).await.expect("Failed to send");

    assert_eventually(|| dbg!(ch2.has_sinks() && ch1.has_sinks())).await;

    ch1.log(&Log::default());
    ch2.log(&Log::default());

    // WS received a message on /2
    let msg = expect_recv!(client, ServerMessage::MessageData);
    assert_eq!(msg.subscription_id, subscription_id);

    // MCAP received a message on /1
    let writer = mcap.close().expect("Failed to close writer");
    let file = writer.into_inner().expect("Failed to get tempfile");
    let summary = read_summary(file.path());
    assert_eq!(summary.channels.len(), 1);
    assert_eq!(
        summary.channels.get(&1).expect("Missing channel 1").topic,
        "/1"
    );
}

use std::io::{BufReader, BufWriter, Read, Seek};

use serde_json::json;
use tempfile::NamedTempFile;

use crate::testutil::assert_eventually;
use crate::websocket::testutil::WebSocketClient;
use crate::websocket::ws_protocol::client::subscribe::Subscription;
use crate::websocket::ws_protocol::client::Subscribe;
use crate::websocket::ws_protocol::server::ServerMessage;
use crate::{ChannelBuilder, Context, McapWriter, Schema, WebSocketServer};

macro_rules! expect_recv {
    ($client:expr, $variant:path) => {{
        let msg = $client.recv().await.expect("Failed to recv");
        match msg {
            $variant(m) => m,
            _ => panic!("Received unexpected message: {msg:?}"),
        }
    }};
}

#[tokio::test]
async fn test_logging_to_file_and_live_sinks() {
    let ctx = Context::new();

    // Configure mcap output
    let mut file = NamedTempFile::new().expect("Create tempfile");

    // Configure live output
    let port = 9998;
    let _ = WebSocketServer::new()
        .bind("127.0.0.1", port)
        .context(&ctx)
        .start()
        .await
        .expect("Failed to start server");

    let mut client = WebSocketClient::connect(format!("127.0.0.1:{port}")).await;

    let channel = ChannelBuilder::new("/test-topic")
        .message_encoding("json")
        .schema(Schema::new(
            "my-schema",
            "jsonschema",
            br#"{
              "type": "object",
              "additionalProperties": true
            }"#,
        ))
        .context(&ctx)
        .build_raw()
        .expect("Failed to create channel");

    {
        // Server info
        expect_recv!(client, ServerMessage::ServerInfo);

        // Advertisement
        let msg = expect_recv!(client, ServerMessage::Advertise);
        let channels = msg.channels;
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].topic, "/test-topic");
        let channel_id = channels[0].id;

        // Client subscription
        client
            .send(&Subscribe::new([Subscription { id: 1, channel_id }]))
            .await
            .expect("Failed to subscribe");

        // Let subscription register before publishing
        assert_eventually(|| dbg!(channel.num_sinks()) == 1).await;
    }

    {
        // Log data to the channel
        let msg = json!({
          "some-key": "some-value"
        })
        .to_string()
        .as_bytes()
        .to_vec();

        // must hold a reference so file is not dropped
        let handle = McapWriter::new()
            .context(&ctx)
            .create(BufWriter::new(file))
            .expect("Failed to record file");

        channel.log(&msg);

        let writer = handle.close().expect("Failed to flush log");
        file = writer
            .into_inner()
            .expect("Failed to get tempfile from bufwriter");
    }

    // Validate mcap output
    file.rewind().expect("Failed to rewind");
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();

    reader
        .read_to_end(&mut buffer)
        .expect("Failed to read file");

    let mut message_count = 0;
    let stream = mcap::MessageStream::new(&buffer).expect("Failed to create message stream");
    for message in stream {
        let message = message.expect("Failed to get message");
        message_count += 1;
        assert_eq!(message.channel.topic, "/test-topic");
        assert_eq!(message.channel.message_encoding, "json");
        assert_ne!(message.log_time, 0);

        let data = String::from_utf8(message.data.to_vec()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(
            json.get("some-key").expect("Missing 'some-key' in json"),
            "some-value"
        );
    }
    assert_eq!(message_count, 1);

    expect_recv!(client, ServerMessage::MessageData);
}

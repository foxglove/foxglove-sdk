use std::io::{BufReader, BufWriter, Read, Seek};

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tempfile::NamedTempFile;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};

use crate::testutil::assert_eventually;
use crate::{ChannelBuilder, Context, McapWriter, Schema, WebSocketServer};

#[tokio::test]
async fn test_logging_to_file_and_live_sinks() {
    let ctx = Context::new();

    // Configure mcap output
    let mut file = NamedTempFile::new().expect("Create tempfile");

    // Configure live output
    let port = 9998;
    let server = WebSocketServer::new()
        .bind("127.0.0.1", port)
        .context(&ctx)
        .start()
        .await
        .expect("Failed to start server");

    let mut ws_client = {
        let mut request = format!("ws://127.0.0.1:{port}/")
            .into_client_request()
            .expect("Failed to build request");

        request.headers_mut().insert(
            "sec-websocket-protocol",
            HeaderValue::from_static("foxglove.sdk.v1"),
        );

        let (ws_stream, _) = tokio_tungstenite::connect_async(request)
            .await
            .expect("Failed to connect");

        ws_stream
    };

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
        .build()
        .expect("Failed to create channel");

    {
        // Server info
        let msg = ws_client
            .next()
            .await
            .expect("No message received")
            .unwrap();
        let json = ws_msg_to_json(msg);
        assert_eq!(json.get("op").expect("Missing 'op'"), "serverInfo");

        // Advertisement
        let msg = ws_client
            .next()
            .await
            .expect("No message received")
            .unwrap();
        let json = ws_msg_to_json(msg);
        let channels = json
            .get("channels")
            .expect("Missing 'channels'")
            .as_array()
            .unwrap();
        assert_eq!(json.get("op").expect("Missing 'op'"), "advertise");
        assert_eq!(channels.len(), 1);
        assert_eq!(
            channels[0].get("topic").expect("Missing topic"),
            "/test-topic"
        );

        // Client subscription
        let channel_id = channels[0].get("id").expect("Missing channel id");
        let subscribe = json!({
            "op": "subscribe",
            "subscriptions": [
                {
                    "id": 1,
                    "channelId": channel_id,
                }
            ]
        });
        ws_client
            .send(Message::text(subscribe.to_string()))
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
        assert_ne!(message.publish_time, 0);

        let data = String::from_utf8(message.data.to_vec()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(
            json.get("some-key").expect("Missing 'some-key' in json"),
            "some-value"
        );
    }
    assert_eq!(message_count, 1);

    let msg = ws_client
        .next()
        .await
        .expect("No message received")
        .expect("Failed to parse message");
    let data = msg.into_data();
    assert_eq!(data[0], 0x01); // message data opcode

    server.stop().await;
}

fn ws_msg_to_json(msg: Message) -> serde_json::Value {
    let data = msg
        .into_text()
        .expect("Failed to convert ws message to text");
    let json: serde_json::Value = serde_json::from_str(&data).unwrap();
    json
}

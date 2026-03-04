//! Integration test: stream an MCAP file with --autoplay and record it back to a new file,
//! then verify the recorded file contains the same channels and messages as the source.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use foxglove::ws_protocol::ParseError;
use foxglove::ws_protocol::client::{Subscribe, Subscription};
use foxglove::ws_protocol::server::ServerMessage;
use foxglove::{
    ChannelBuilder, Context, McapWriter, PartialMetadata, RawChannel, Schema,
    WebSocketClient, WebSocketClientError,
};
use mcap::sans_io::indexed_reader::{IndexedReadEvent, IndexedReader, IndexedReaderOptions};
use mcap::sans_io::summary_reader::{SummaryReadEvent, SummaryReader};

/// Binds port 0 to get a free port from the OS, then drops the listener.
fn find_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Kill the child process on drop so the server doesn't outlive the test.
struct KillOnDrop(std::process::Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

/// Create a small MCAP with known messages. Returns `(topic, log_time_ns, data)` for each
/// message written, in order.
fn create_test_mcap(path: &Path) -> Vec<(String, u64, Vec<u8>)> {
    let ctx = Arc::new(Context::new());
    let mcap = McapWriter::new()
        .context(&ctx)
        .create_new_buffered_file(path)
        .expect("create source mcap");

    let channel = ChannelBuilder::new("/test")
        .message_encoding("json")
        .context(&ctx)
        .build_raw()
        .expect("build channel");

    let payloads: &[&[u8]] = &[b"{\"v\":1}", b"{\"v\":2}", b"{\"v\":3}"];
    // Use 1 ms gaps so the stream server emits everything almost instantly.
    let base_ns: u64 = 1_000_000_000;
    let mut expected = Vec::new();
    for (i, &data) in payloads.iter().enumerate() {
        let log_time = base_ns + i as u64 * 1_000_000;
        channel.log_with_meta(data, PartialMetadata { log_time: Some(log_time) });
        expected.push(("/test".to_string(), log_time, data.to_vec()));
    }

    mcap.close().expect("close source mcap");
    expected
}

/// Read all messages from an MCAP, returning `topic -> [(log_time, data)]` in time order.
fn read_mcap_messages(path: &Path) -> BTreeMap<String, Vec<(u64, Vec<u8>)>> {
    let mut file = BufReader::new(std::fs::File::open(path).unwrap());

    let mut sr = SummaryReader::new();
    while let Some(event) = sr.next_event() {
        match event.expect("summary read") {
            SummaryReadEvent::ReadRequest(n) => {
                let read = file.read(sr.insert(n)).unwrap();
                sr.notify_read(read);
            }
            SummaryReadEvent::SeekRequest(pos) => {
                let pos = file.seek(pos).unwrap();
                sr.notify_seeked(pos);
            }
        }
    }
    let summary = sr.finish().expect("summary missing");

    let mut reader = IndexedReader::new_with_options(&summary, IndexedReaderOptions::new())
        .expect("indexed reader");
    let mut chunk_buf = Vec::new();
    let mut out: BTreeMap<String, Vec<(u64, Vec<u8>)>> = BTreeMap::new();

    loop {
        match reader.next_event() {
            None => break,
            Some(Err(e)) => panic!("mcap read error: {e}"),
            Some(Ok(IndexedReadEvent::ReadChunkRequest { offset, length })) => {
                file.seek(SeekFrom::Start(offset)).unwrap();
                chunk_buf.resize(length, 0);
                file.read_exact(&mut chunk_buf).unwrap();
                reader
                    .insert_chunk_record_data(offset, &chunk_buf)
                    .expect("insert chunk");
            }
            Some(Ok(IndexedReadEvent::Message { header, data })) => {
                let topic = summary.channels[&header.channel_id].topic.clone();
                out.entry(topic).or_default().push((header.log_time, data.to_vec()));
            }
        }
    }

    out
}

/// Connect to the WebSocket server and write all incoming messages to `output` until the
/// server closes the connection.
async fn record_to_file(addr: &str, output: &Path) -> anyhow::Result<()> {
    let ctx = Arc::new(Context::new());
    let mcap = McapWriter::new()
        .context(&ctx)
        .create_new_buffered_file(output)?;

    let mut client = WebSocketClient::connect(addr).await?;

    match client.recv().await? {
        ServerMessage::ServerInfo(_) => {}
        msg => anyhow::bail!("expected ServerInfo, got {msg:?}"),
    }

    let mut server_channels: HashMap<u64, (u32, Arc<RawChannel>)> = HashMap::new();
    let mut subscriptions: HashMap<u32, Arc<RawChannel>> = HashMap::new();
    let mut next_sub_id: u32 = 0;

    loop {
        let mut pending_subs: Vec<Subscription> = Vec::new();

        let msg = match client.recv().await {
            Ok(msg) => msg,
            Err(WebSocketClientError::Timeout(_)) => continue,
            Err(WebSocketClientError::UnexpectedEndOfStream)
            | Err(WebSocketClientError::ParseError(ParseError::UnhandledMessageType)) => break,
            Err(e) => return Err(e.into()),
        };

        match msg {
            ServerMessage::Advertise(advertise) => {
                for adv_ch in advertise.channels {
                    if server_channels.contains_key(&adv_ch.id) {
                        continue;
                    }
                    let topic = adv_ch.topic.to_string();
                    let encoding = adv_ch.encoding.to_string();
                    let schema = decode_schema(&adv_ch);
                    let ch_id = adv_ch.id;

                    let channel = ChannelBuilder::new(&topic)
                        .message_encoding(encoding)
                        .schema(schema)
                        .context(&ctx)
                        .build_raw()?;

                    let sub_id = next_sub_id;
                    next_sub_id += 1;
                    pending_subs.push(Subscription::new(sub_id, ch_id));
                    subscriptions.insert(sub_id, channel.clone());
                    server_channels.insert(ch_id, (sub_id, channel));
                }
            }
            ServerMessage::MessageData(msg) => {
                if let Some(ch) = subscriptions.get(&msg.subscription_id) {
                    ch.log_with_meta(
                        &msg.data,
                        PartialMetadata { log_time: Some(msg.log_time) },
                    );
                }
            }
            _ => {}
        }

        if !pending_subs.is_empty() {
            client.send(&Subscribe::new(pending_subs)).await?;
        }
    }

    mcap.close()?;
    Ok(())
}

fn decode_schema(adv_ch: &foxglove::ws_protocol::server::Channel<'_>) -> Option<Schema> {
    let schema_encoding = adv_ch.schema_encoding.as_deref()?;
    if schema_encoding.is_empty() {
        return None;
    }
    let schema_data = adv_ch.decode_schema().ok()?;
    Some(Schema::new(
        adv_ch.schema_name.as_ref().to_owned(),
        schema_encoding.to_owned(),
        schema_data,
    ))
}

#[tokio::test]
async fn test_stream_and_record_roundtrip() {
    let tmpdir = tempfile::tempdir().unwrap();
    let source_path = tmpdir.path().join("source.mcap");
    let output_path = tmpdir.path().join("recording.mcap");

    // Create a small source MCAP.
    create_test_mcap(&source_path);

    // Spawn the stream server with --autoplay on a free port. It will shut down automatically
    // when playback finishes.
    let port = find_free_port();
    let server_proc = std::process::Command::new(env!("STREAM_SERVER_EXE"))
        .args([
            "--autoplay",
            "--port",
            &port.to_string(),
            "--file",
            source_path.to_str().unwrap(),
        ])
        .spawn()
        .expect("failed to spawn stream server");
    let _guard = KillOnDrop(server_proc);

    // Wait for the server to start accepting connections (retry for up to 10 s).
    // Close the probe connection gracefully to avoid broken-pipe errors in server logs.
    let addr = format!("127.0.0.1:{port}");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "stream server did not start within 10 s"
        );
        if let Ok(mut client) = WebSocketClient::connect(&addr).await {
            // Drain ServerInfo before closing so the server isn't mid-send when we disconnect.
            let _ = client.recv().await;
            let _ = client.close().await;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Run the record-client logic; it exits when the server closes the connection.
    record_to_file(&addr, &output_path)
        .await
        .expect("record_to_file failed");

    // Compare channels and messages.
    let source_msgs = read_mcap_messages(&source_path);
    let recorded_msgs = read_mcap_messages(&output_path);

    assert_eq!(
        source_msgs, recorded_msgs,
        "recorded MCAP should contain the same messages as the source"
    );
}

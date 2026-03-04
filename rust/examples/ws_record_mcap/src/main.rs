//! Subscribes to a Foxglove WebSocket server and records messages to an MCAP file.
//!
//! Connects to a running Foxglove WebSocket server, subscribes to all advertised topics
//! (or a filtered subset), and writes incoming messages to an MCAP file. The file is
//! finalized and saved when the process receives Ctrl-C.
//!
//! Usage:
//! ```text
//! cargo run -p example_ws_record_mcap -- --addr 127.0.0.1:8765 --output recording.mcap
//! cargo run -p example_ws_record_mcap -- --addr 127.0.0.1:8765 --output recording.mcap --topic /pose --topic /imu
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context as _, Result};
use clap::Parser;
use foxglove::ws_protocol::client::{Subscribe, Subscription};
use foxglove::ws_protocol::server::ServerMessage;
use foxglove::{ChannelBuilder, Context, McapWriter, PartialMetadata, RawChannel, Schema};
use foxglove::WebSocketClientError;
use tracing::info;

#[derive(Debug, Parser)]
#[command(about = "Record a Foxglove WebSocket stream to an MCAP file")]
struct Cli {
    /// WebSocket server address (host:port).
    #[arg(long, default_value = "127.0.0.1:8765")]
    addr: String,

    /// Output MCAP file path.
    #[arg(short, long, default_value = "output.mcap")]
    output: PathBuf,

    /// Topics to subscribe to. May be specified multiple times.
    /// If not specified, all advertised topics are recorded.
    #[arg(short, long)]
    topic: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    let args = Cli::parse();

    // Use an isolated context so channels and sinks don't pollute the global default.
    let ctx = Arc::new(Context::new());

    // Open the MCAP file before connecting, so it's ready to record from the first message.
    let mcap = McapWriter::new()
        .context(&ctx)
        .create_new_buffered_file(&args.output)
        .with_context(|| format!("Failed to create {:?}", args.output))?;

    info!("Recording to {:?}", args.output);

    // Connect to the Foxglove WebSocket server.
    let mut client = foxglove::WebSocketClient::connect(&args.addr)
        .await
        .with_context(|| format!("Failed to connect to ws://{}", args.addr))?;

    info!("Connected to ws://{}", args.addr);

    // Expect the ServerInfo handshake before anything else.
    match client.recv().await? {
        ServerMessage::ServerInfo(info) => {
            info!("Server: {}", info.name);
        }
        msg => bail!("Expected ServerInfo, got {msg:?}"),
    }

    // server channel_id  ->  (subscription_id, local RawChannel)
    let mut server_channels: HashMap<u64, (u32, Arc<RawChannel>)> = HashMap::new();
    // subscription_id  ->  local RawChannel  (fast lookup when messages arrive)
    let mut subscriptions: HashMap<u32, Arc<RawChannel>> = HashMap::new();
    let mut next_sub_id: u32 = 0;

    // Arrange for Ctrl-C to break out of the recording loop.
    let shutdown = async {
        tokio::signal::ctrl_c().await.ok();
    };
    tokio::pin!(shutdown);

    loop {
        // Pending subscribe requests built up while processing an Advertise batch.
        let mut pending_subs: Vec<Subscription> = Vec::new();

        tokio::select! {
            biased;

            _ = &mut shutdown => {
                info!("Shutting down...");
                break;
            }

            result = client.recv() => {
                let msg = match result {
                    Ok(msg) => msg,
                    // recv() has a 1-second internal timeout; just retry.
                    Err(WebSocketClientError::Timeout(_)) => continue,
                    Err(WebSocketClientError::UnexpectedEndOfStream) => {
                        info!("Server closed the connection");
                        break;
                    }
                    Err(e) => return Err(e.into()),
                };

                match msg {
                    ServerMessage::Advertise(advertise) => {
                        for adv_ch in advertise.channels {
                            let topic = adv_ch.topic.to_string();

                            // Skip topics that don't match the filter (if one was provided).
                            if !args.topic.is_empty()
                                && !args.topic.iter().any(|t| *t == topic)
                            {
                                continue;
                            }

                            // Skip channels we're already subscribed to.
                            if server_channels.contains_key(&adv_ch.id) {
                                continue;
                            }

                            let schema = decode_schema(&adv_ch);
                            let encoding = adv_ch.encoding.to_string();
                            let ch_id = adv_ch.id;

                            let channel = ChannelBuilder::new(topic.as_str())
                                .message_encoding(encoding)
                                .schema(schema)
                                .context(&ctx)
                                .build_raw()
                                .with_context(|| {
                                    format!("Failed to create channel for {topic}")
                                })?;

                            let sub_id = next_sub_id;
                            next_sub_id += 1;

                            info!("Subscribing to {topic} (sub_id={sub_id})");
                            pending_subs.push(Subscription::new(sub_id, ch_id));
                            subscriptions.insert(sub_id, channel.clone());
                            server_channels.insert(ch_id, (sub_id, channel));
                        }
                    }

                    ServerMessage::Unadvertise(unadvertise) => {
                        for ch_id in unadvertise.channel_ids {
                            if let Some((sub_id, _)) = server_channels.remove(&ch_id) {
                                subscriptions.remove(&sub_id);
                                info!("Channel {ch_id} unadvertised");
                            }
                        }
                    }

                    ServerMessage::MessageData(msg) => {
                        if let Some(channel) = subscriptions.get(&msg.subscription_id) {
                            channel.log_with_meta(
                                &msg.data,
                                PartialMetadata {
                                    log_time: Some(msg.log_time),
                                },
                            );
                        }
                    }

                    _ => {}
                }
            }
        }

        // Send any new subscriptions accumulated during this iteration. This is done outside the
        // select! block so that `client` is not borrowed by the received message at the same time.
        if !pending_subs.is_empty() {
            client.send(&Subscribe::new(pending_subs)).await?;
        }
    }

    mcap.close().context("Failed to finalize MCAP file")?;
    info!("Saved {:?}", args.output);

    Ok(())
}

/// Decodes the schema from an advertised channel, if present.
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

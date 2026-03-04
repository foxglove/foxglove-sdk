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

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context as _, Result};
use clap::Parser;
use foxglove::ws_protocol::server::ServerMessage;
use foxglove::{Context, McapWriter};
use tracing::info;

use example_ws_record_mcap::record_stream;

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

    let shutdown = async { let _ = tokio::signal::ctrl_c().await; };
    tokio::pin!(shutdown);

    let msg_count = record_stream(&mut client, &ctx, &args.topic, shutdown).await?;

    mcap.close().context("Failed to finalize MCAP file")?;
    info!("Saved {:?} ({msg_count} messages)", args.output);

    Ok(())
}

# WebSocket to MCAP Recorder

Connects to a Foxglove WebSocket server, subscribes to topics, and writes all incoming messages to an MCAP file. The file is finalized and saved when the process receives Ctrl-C.

This is the inverse of the [`ws_stream_mcap`](../ws_stream_mcap) example, which reads an MCAP file and streams it over WebSocket.

## Usage

```bash
# Record all topics from a local server
cargo run -p example_ws_record_mcap -- --output recording.mcap

# Connect to a different address
cargo run -p example_ws_record_mcap -- --addr 192.168.1.10:8765 --output recording.mcap

# Record only specific topics
cargo run -p example_ws_record_mcap -- --output recording.mcap --topic /pose --topic /imu

# Use LZ4 compression with a 10MB chunk size
cargo run -p example_ws_record_mcap -- --output recording.mcap --compression lz4 --chunk-size 10485760

# Disable compression
cargo run -p example_ws_record_mcap -- --output recording.mcap --compression none
```

Press **Ctrl-C** to stop recording. The MCAP file is flushed and finalized on shutdown.

## Options

| Flag | Default | Description |
|------|---------|-------------|
| `--addr` | `127.0.0.1:8765` | WebSocket server address (`host:port`) |
| `--output` / `-o` | `output.mcap` | Output MCAP file path |
| `--topic` / `-t` | _(all topics)_ | Topic to subscribe to; repeat to record multiple topics |
| `--compression` | `zstd` | Compression algorithm: `zstd`, `lz4`, or `none` |
| `--chunk-size` | `5242880` (5 MB) | Chunk size in bytes |

## How It Works

1. Opens the output MCAP file
2. Connects to the WebSocket server and waits for the `ServerInfo` handshake
3. For each `Advertise` message from the server, creates a local channel and sends a `Subscribe` request
4. Writes every incoming `MessageData` frame to the MCAP file, preserving the original `log_time`
5. Handles `Unadvertise` messages to stop recording channels that disappear
6. On Ctrl-C, finalizes the MCAP file (writes the summary section and footer)

Channel schemas are decoded from the advertised channel metadata and stored in the MCAP file, so the recording is self-contained and can be opened directly in Foxglove.

## Testing

An integration test in the [`ws_stream_mcap`](../ws_stream_mcap) example verifies that the
recorder captures the same messages the stream server emits:

```bash
cargo test -p example_ws_record_mcap --test roundtrip_test
```

# MCAP to WebSocket Streamer

Reads an MCAP file and streams its messages over a Foxglove WebSocket server with full playback control — play, pause, seek, and variable speed.

This is the inverse of the [`ws_record_mcap`](../ws_record_mcap) example, which connects to a WebSocket server and records messages to an MCAP file.

## Usage

```bash
# Stream an MCAP file on the default address (paused; use Foxglove to press Play)
cargo run -p example_ws_stream_mcap -- --file recording.mcap

# Stream on a custom host and port
cargo run -p example_ws_stream_mcap -- --file recording.mcap --host 0.0.0.0 --port 9000

# Start playing immediately and shut down when the file ends (useful with ws_record_mcap)
cargo run -p example_ws_stream_mcap -- --file recording.mcap --autoplay
```

Open `ws://127.0.0.1:8765` in [Foxglove](https://app.foxglove.dev) to connect. Use the playback controls in Foxglove to play, pause, seek, and adjust speed.

Press **Ctrl-C** to stop the server.

## Options

| Flag | Default | Description |
|------|---------|-------------|
| `--file` / `-f` | _(required)_ | MCAP file to stream |
| `--host` | `127.0.0.1` | Server bind address |
| `--port` / `-p` | `8765` | Server TCP port |
| `--autoplay` | `false` | Start playing immediately; shut down when playback ends |

## How It Works

1. Reads the MCAP summary section to discover channels, schemas, and the time range of the recording
2. Creates a Foxglove WebSocket server advertising `PlaybackControl` and `Time` capabilities
3. Waits for a client to connect, then enters a playback loop
4. On each iteration, calls `log_next_message` which:
   - Checks whether the next message's timestamp is due based on wall-clock time and playback speed
   - Logs the message to the appropriate channel, or returns a sleep duration if it's not yet time
   - Periodically broadcasts a time update (~60 Hz) so Foxglove's timeline stays in sync
5. Responds to `PlaybackControlRequest` messages from Foxglove to handle play/pause, seek, and speed changes
6. Broadcasts a `PlaybackState` update when playback reaches the end of the file

### Architecture

The example is structured around a `PlaybackSource` trait that decouples playback control logic from the MCAP format. `McapPlayer` implements this trait, while `main.rs` wires it together with the WebSocket server listener. You can implement `PlaybackSource` for your own data format and reuse the same playback loop structure.

# Foxglove Rust SDK

## Development

### Dependencies

- Rust, installed via [rustup](https://rustup.rs/)
- The python environment described in the [Python SDK README](../python/foxglove-sdk/README.md)

### Generate Protobuf schemas

```bash
cargo run --bin foxglove_proto_gen
```

### Run the example server

1. `cargo run -p example_ws_server`
2. Open the Foxglove desktop app
3. From the dashboard, click "Open connection..."
4. Confirm the WebSocket URL and click "Open"
5. Create a Raw Messages panel and enter "topic" in the message path field at the top

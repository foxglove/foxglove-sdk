# MCAP playback control

An example from the Foxglove SDK.

This example reads the given MCAP file and streams the data to a Foxglove WebSocket
server, exposing the playback control capability so Foxglove can play, pause, seek,
and change playback speed.

## Usage

This example uses [uv](https://docs.astral.sh/uv/).

```bash
uv run python main.py --file /path/to/file.mcap
```

# Logging Context

An example from the Foxglove SDK.

This demonstrates creating multiple logging contexts in order to log a selection of topics to
different MCAP sinks. It creates two files, "file1.mcap" and "file2.mcap" in the current directory
and writes some data to each. You can inspect the written channels using `mcap info {file}` from the
[MCAP CLI](https://mcap.dev/guides/cli).

## Usage

This example uses [uv](https://docs.astral.sh/uv/).

```bash
uv run python main.py
```

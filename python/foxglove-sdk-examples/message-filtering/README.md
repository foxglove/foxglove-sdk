# Message filtering

An example from the Foxglove SDK.

This example demonstrates how to use the Foxglove SDK to filter messages when logging to an MCAP
file and/or a WebSocket server.

Oftentimes, you may want to split "heavy" topics out into separate MCAP recordings, but still log
everything for live visualization. Splitting on topic in this way can be useful for selectively
retrieving data from bandwidth-constrained environments, such as with the Foxglove Agent.

In this example, we log some point cloud data to one MCAP file, and some minimal metadata to
another.

## Usage

This example uses Poetry: https://python-poetry.org/

```bash
poetry install
poetry run python main.py
```

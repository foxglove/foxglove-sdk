# Python logging configuration

An example from the Foxglove SDK.

The `foxglove` module provides a `set_log_level` function for convenience in scripts, but for more
involved applications you'll likely want to configure logging yourself. Most examples use the former
for simplicity; this example demonstrates some logging configuration which might be more typical of
real-world usage.

## Usage

This example uses Poetry: https://python-poetry.org/

```bash
poetry install
poetry run python main.py [--path file.mcap]
```

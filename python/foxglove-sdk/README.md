# Foxglove Python SDK

The official [Foxglove](https://docs.foxglove.dev/docs) SDK for Python.

This package provides support for integrating with the Foxglove platform. It can be used to log
events to local [MCAP](https://mcap.dev/) files or a local visualization server that communicates
with the Foxglove app.

## Get Started

See https://foxglove.github.io/foxglove-sdk/python/

## Requirements

- Python 3.9+

## Examples

We're using uv as a Python package manager in the foxglove-sdk-examples.

To test that all examples run (as the CI does) you can use `yarn run-python-sdk-examples` in the repo root.

To run a specific example (e.g. write-mcap-file) with local changes:

```
cd python/foxglove-sdk-examples/write-mcap-file
uv run --with ../../foxglove-sdk main.py [args]
```

Keep in mind that uv does two layers of caching.
There's the .venv in your project directory, plus a global cache at ~/.cache/uv.

uv tries to be smart about not rebuilding things it has already built,
which means that if you make changes and you want them to show up,
you also need to run `uv cache clean`.

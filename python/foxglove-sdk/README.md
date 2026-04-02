# Foxglove Python SDK

The official [Foxglove](https://docs.foxglove.dev/docs) SDK for Python.

This package provides support for integrating with the Foxglove platform. It can be used to log
events to local [MCAP](https://mcap.dev/) files or a local visualization server that communicates
with the Foxglove app.

## Get Started

See https://foxglove-sdk-api-docs.pages.dev/python/

## Requirements

- Python 3.10+

## Remote Access

The SDK includes optional support for remote access, which enables live visualization and teleop
through the Foxglove platform via a gateway connection.

Remote access is available on the following platforms:

| Platform       | Architecture | Remote Access |
|----------------|-------------|---------------|
| Linux (glibc)  | x86_64      | Yes (manylinux_2_28, glibc >= 2.28) |
| Linux (glibc)  | aarch64     | Yes (manylinux_2_28, glibc >= 2.28) |
| Linux (glibc)  | x86, armv7, s390x, ppc64le | No |
| Linux (musl)   | all         | No            |
| macOS          | aarch64     | Yes           |
| macOS          | x86_64      | Yes           |
| Windows        | x86_64      | Yes           |
| Windows        | x86         | No            |

On supported platforms, pre-built wheels include remote access. On unsupported platforms the SDK
works normally but `foxglove.start_gateway` will not be available.

When building from source, remote access must be enabled explicitly:

```
MATURIN_PEP517_ARGS="--features remote-access" pip install .
```

This requires system dependencies including `libva-dev` (Linux) and a C++ toolchain.

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

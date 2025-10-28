# Foxglove Python SDK

## Development

### Installation

We use [uv](https://docs.astral.sh/uv/getting-started/installation/) to manage dependencies.

### Developing

Prefix python commands with `uv run`. For more details, refer to the [uv docs](https://docs.astral.sh/uv/).

Before running any of the following commands, you should ensure you have a local
venv and install dev dependencies. There's no need to explicitly activate the
venv; `uv` will use it automatically.

```sh
uv sync --all-extras
```

After making changes, you can install the SDK into the local venv with the
following command. this is an [editable
install](https://setuptools.pypa.io/en/latest/userguide/development_mode.html),
so you don't need to reinstall after making changes to python sources. If you
make changes to rust sources, however, you do need to reinstall.

```sh
uv pip install --editable .
```

To test the [Jupyter](https://jupyter.org) integration:

```sh
# Install the SDK with the notebook extra.
uv pip install --editable '.[notebook]'

# Install Jupyter lab.
uv pip install jupyterlab

# Launch Jupyter lab.
uv run jupyter lab
```

To check types, run:

```sh
uv run mypy .
```

Format code:

```sh
uv run black .
```

PEP8 check:

```sh
uv run flake8 .
```

Run unit tests:

```sh
uv pip install -e '.[notebook]'
uv run pytest
```

Benchmark tests should be marked with `@pytest.mark.benchmark`. These are not run by default.

```sh
uv pip install -e '.[notebook]'

# to run with benchmarks
uv run pytest --with-benchmarks

# to run only benchmarks
uv run pytest -m benchmark
```

### Examples

Examples exist in the `foxglove-sdk-examples` directory. See each example's readme for usage.

### Documentation

Sphinx documentation can be generated from this directory with:

```sh
uv run sphinx-build ./python/docs ./python/docs/_build
```

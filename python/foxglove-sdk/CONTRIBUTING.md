# Foxglove Python SDK

## Development

### Installation

We use [uv](https://docs.astral.sh/uv/getting-started/installation/) to manage dependencies.

### Developing

Prefix python commands with `uv run`. For more details, refer to the [uv docs](https://docs.astral.sh/uv/).

After making changes to rust code, rebuild and install it with:

```sh
uv pip install -e .
```

To test the notebook integration:

```sh
# Install Jupyter and SDK with notebook extra
uv pip install jupyterlab -e '.[notebook]'

# Launch jupyter lab
uv run jupyter lab
```

To check types, run:

```sh
uv sync --all-extras
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

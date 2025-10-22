# Foxglove Python SDK

## Development

### Installation

We use [uv](https://docs.astral.sh/uv/getting-started/installation/) to manage dependencies.

Install front-end dependencies

```sh
yarn install
```

Build front-end

```sh
yarn workspace @foxglove/notebook-frontend build
```

### Developing

Prefix python commands with `uv run`. For more details, refer to the [uv docs](https://docs.astral.sh/uv/).

After making changes to rust code, rebuild with:

```sh
uv run maturin develop
```

After making changes to the typescript code, rebuild with:

```sh
yarn build
```

To test the notebook integration:

```sh
# Install Jupyter
uv pip install jupyterlab

# Build SDK with notebook extra
uv run maturin develop -E notebook

# launch jupyter lab
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
uv run pytest
```

Benchmark tests should be marked with `@pytest.mark.benchmark`. These are not run by default.

```sh
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

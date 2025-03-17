# Foxglove Python SDK

## Development

### Installation

We use [Poetry](https://python-poetry.org/) to manage dependencies.

Install Poetry:

```sh
brew install pipx
pipx ensurepath
pipx install poetry
```

Install dependencies

```sh
poetry install
```

### Developing

To make use of installed dependencies, prefix python commands with `poetry run`. For more details, refer to the [Poetry docs](https://python-poetry.org/docs/basic-usage/).

After making changes to rust code, rebuild with:

```sh
poetry run maturin develop
```

To check types, run:

```sh
poetry run mypy .
```

Format code:

```sh
poetry run black .
```

PEP8 check:

```sh
poetry run flake8 .
```

Run unit tests:

```sh
poetry run pytest
```

Benchmark tests should be marked with `@pytest.mark.benchmark`. These are not run by default.

```sh
# to run with benchmarks
poetry run pytest --with-benchmarks

# to run only benchmarks
poetry run pytest -m benchmark
```

### Examples

Examples exist in the `foxglove-sdk-examples` directotry. See each example's readme for usage.

### Documentation

Sphinx documentation can be generated from this directory with:

```sh
poetry run sphinx-build ./python/docs ./python/docs/_build
```

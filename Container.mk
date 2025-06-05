.PHONY: generate
generate:
	poetry install
	yarn generate

.PHONY: lint-python
lint-python:
	poetry check --strict
	poetry install
	poetry run black . --check
	poetry run isort . --check
	poetry run flake8 .

.PHONY: test-python
test-python:
	poetry -C python/foxglove-sdk check --strict
	poetry -C python/foxglove-sdk install
	poetry -C python/foxglove-sdk run maturin develop
	poetry -C python/foxglove-sdk run mypy .
	poetry -C python/foxglove-sdk run pytest

.PHONY: benchmark-python
benchmark-python:
	poetry -C python/foxglove-sdk run pytest --with-benchmarks

.PHONY: lint-rust
lint-rust:
	cargo fmt --all --check
	cargo clippy --no-deps --all-targets --tests -- -D warnings

.PHONY: build-rust
build-rust:
	cargo build --all-targets

.PHONY: build-rust-foxglove-msrv
build-rust-foxglove-msrv:
	cargo +1.83.0 build -p foxglove --all-features

.PHONY: test-rust
test-rust:
	cargo test --all-features

.PHONY: test-rust-foxglove-no-default-features
test-rust-foxglove-no-default-features:
	cargo test -p foxglove --no-default-features

.PHONY: rust-docs
rust-docs:
	cargo +nightly rustdoc -p foxglove --all-features -- -D warnings --cfg docsrs

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
	poetry check --strict
	poetry install
	poetry run maturin develop
	poetry run mypy .
	poetry run pytest --with-benchmarks

.PHONY: lint-rust
lint-rust:
	cargo fmt --all --check
	cargo clippy --no-deps --all-targets --tests -- -D warnings

.PHONY: build-rust
build-rust:
	cargo build --all-targets

.PHONY: test-rust
test-rust:
	cargo test --all-features

.PHONY: test-rust-foxglove-no-default-features
test-rust-foxglove-no-default-features:
	cargo test -p foxglove --no-default-features

.PHONY: rust-docs
rust-docs:
	cargo +nightly rustdoc -p foxglove --all-features -- -D warnings --cfg docsrs

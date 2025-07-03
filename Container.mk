.PHONY: generate
generate:
	poetry install
	yarn install
	yarn generate

.PHONY: build-python
build-python:
	poetry -C python/foxglove-sdk check --strict
	poetry -C python/foxglove-sdk install
	poetry -C python/foxglove-sdk run maturin develop

.PHONY: lint-python
lint-python:
	poetry check --strict
	poetry install
	poetry run black python --check
	poetry run isort python --check
	poetry run flake8 python

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
	cargo +$(MSRV_RUST_VERSION) build -p foxglove --all-features

.PHONY: test-rust
test-rust:
	cargo test --all-features

.PHONY: test-rust-foxglove-no-default-features
test-rust-foxglove-no-default-features:
	cargo test -p foxglove --no-default-features

.PHONY: docs-rust
docs-rust:
	cargo +nightly rustdoc -p foxglove --all-features -- -D warnings --cfg docsrs

.PHONY: clean-cpp
clean-cpp:
	rm -rf cpp/build*

.PHONY: clean-docs-cpp
clean-docs-cpp:
	rm -rf cpp/foxglove/docs/generated
	rm -rf cpp/build/docs

.PHONY: docs-cpp
docs-cpp: clean-docs-cpp
	poetry install -C cpp/foxglove/docs
	make -C cpp docs

.PHONY: build-cpp
build-cpp:
	make -C cpp build

.PHONY: build-cpp-tidy
build-cpp-tidy:
	make -C cpp CLANG_TIDY=true build

.PHONY: lint-cpp
lint-cpp:
	make -C cpp lint

.PHONY: lint-fix-cpp
lint-fix-cpp:
	make -C cpp lint-fix

.PHONY: test-cpp
test-cpp:
	make -C cpp test

.PHONY: test-cpp-sanitize
test-cpp-sanitize:
	make -C cpp SANITIZE=address,undefined test

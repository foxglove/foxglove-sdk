.PHONY: generate
generate:
	yarn install
	yarn generate

.PHONY: build-python
build-python:
	uv --directory python/foxglove-sdk lock --check
	uv --directory python/foxglove-sdk sync --all-extras
	uv --directory python/foxglove-sdk pip install --editable '.[notebook]'

.PHONY: lint-python
lint-python:
	uv lock --check
	uv run black python --check
	uv run isort python --check
	uv run flake8 python

.PHONY: test-python
test-python:
	uv --directory python/foxglove-sdk lock --check
	uv --directory python/foxglove-sdk sync --all-extras
	uv --directory python/foxglove-sdk pip install --editable '.[notebook]'
	uv --directory python/foxglove-sdk run mypy .
	uv --directory python/foxglove-sdk run pytest

.PHONY: benchmark-python
benchmark-python:
	uv --directory python/foxglove-sdk lock --check
	uv --directory python/foxglove-sdk sync --all-extras
	uv --directory python/foxglove-sdk pip install --editable '.[notebook]'
	uv --directory python/foxglove-sdk run pytest --with-benchmarks

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

.PHONY: generate
generate:
	yarn install
	yarn generate

PYTHON_REMOTE_ACCESS ?= ON
ifeq ($(PYTHON_REMOTE_ACCESS),ON)
FOXGLOVE_TEST_REQUIRE_REMOTE_ACCESS = 1
MATURIN_PEP517_ARGS += --features remote-access
else
FOXGLOVE_TEST_REQUIRE_REMOTE_ACCESS = 0
endif

# Opts into a build-time check that NVENC hardware acceleration for video
# encoding will be available (cuda.h is present on supported targets). Only
# meaningful when remote-access is also enabled. Defaults to OFF because the
# check fails the build on hosts without the CUDA toolkit; opt in explicitly
# (e.g. in CI) where you want the loud failure.
PYTHON_REQUIRE_CUDA ?= OFF
ifeq ($(PYTHON_REQUIRE_CUDA),ON)
MATURIN_PEP517_ARGS += --features require-cuda
endif

.PHONY: build-python
build-python:
	uv --directory python/foxglove-sdk lock --check
	uv --directory python/foxglove-sdk sync --all-extras
	MATURIN_PEP517_ARGS="$(MATURIN_PEP517_ARGS)" uv --directory python/foxglove-sdk pip install --editable '.[notebook]'

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
	MATURIN_PEP517_ARGS="$(MATURIN_PEP517_ARGS)" uv --directory python/foxglove-sdk pip install --editable '.[notebook]'
	uv --directory python/foxglove-sdk run mypy .
	FOXGLOVE_TEST_REQUIRE_REMOTE_ACCESS="$(FOXGLOVE_TEST_REQUIRE_REMOTE_ACCESS)" uv --directory python/foxglove-sdk run pytest

.PHONY: benchmark-python
benchmark-python:
	uv --directory python/foxglove-sdk lock --check
	uv --directory python/foxglove-sdk sync --all-extras
	MATURIN_PEP517_ARGS="$(MATURIN_PEP517_ARGS)" uv --directory python/foxglove-sdk pip install --editable '.[notebook]'
	uv --directory python/foxglove-sdk run pytest --with-benchmarks

.PHONY: docs-python
docs-python:
	uv --directory python/foxglove-sdk lock --check
	uv --directory python/foxglove-sdk sync --all-extras
	MATURIN_PEP517_ARGS="$(MATURIN_PEP517_ARGS)" uv --directory python/foxglove-sdk pip install --editable '.[notebook]'
	uv --directory python/foxglove-sdk run sphinx-build --fail-on-warning ./python/docs ./python/docs/_build

.PHONY: clean-docs-python
clean-docs-python:
	rm -rf python/foxglove-sdk/python/docs/_build

.PHONY: generate-python-schemas-flatbuffer
generate-python-schemas-flatbuffer:
	make -C python generate-flatbuffer

.PHONY: generate-python-schemas-protobuf
generate-python-schemas-protobuf:
	make -C python generate-protobuf

.PHONY: generate-python-schemas
generate-python-schemas:
	make -C python generate

.PHONY: build-python-schemas
build-python-schemas:
	make -C python build

.PHONY: test-python-schemas
test-python-schemas:
	make -C python test

.PHONY: clean-python-schemas
clean-python-schemas:
	make -C python clean

.PHONY: lint-rust
lint-rust:
	cargo fmt --all --check
	cargo clippy --no-deps --all-targets --tests -- -D warnings

.PHONY: build-rust
build-rust:
	cargo build --all-targets

.PHONY: build-rust-foxglove-msrv
build-rust-foxglove-msrv:
	cargo +$(MSRV_RUST_VERSION) build -p foxglove --features full

.PHONY: test-rust
test-rust:
	cargo test -p foxglove --features full
	cargo test -p foxglove_c --features full
	cargo test -p foxglove_data_loader
	cargo test -p foxglove_derive
	cargo test -p foxglove-sdk-python --features full

.PHONY: test-rust-foxglove-no-default-features
test-rust-foxglove-no-default-features:
	cargo test -p foxglove --no-default-features

.PHONY: docs-rust
docs-rust:
	cargo +nightly rustdoc -p foxglove --features full -- -D warnings --cfg docsrs

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
	make -C cpp SANITIZE=address,undefined FOXGLOVE_REMOTE_ACCESS=OFF test

# Build the C/C++ SDK into a directory suitable for use as
# FETCHCONTENT_SOURCE_DIR_FOXGLOVE_SDK in CMake. The recipe lives in
# cpp/Makefile; CPP_SDK_DIR is resolved to an absolute path here so that
# root-relative overrides (e.g. CPP_SDK_DIR=artifacts/foxglove in CI) still
# land where the caller expects despite the sub-make running in cpp/.
CPP_SDK_DIR ?= cpp/dist
.PHONY: build-cpp-dist
build-cpp-dist:
	make -C cpp CPP_SDK_DIR=$(abspath $(CPP_SDK_DIR)) build-dist

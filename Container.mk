.PHONY: generate
generate:
	yarn install
	yarn generate

PYTHON_REMOTE_ACCESS ?= ON

ifeq ($(PYTHON_REMOTE_ACCESS),ON)
MATURIN_PEP517_ARGS += --features remote-access
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
	uv --directory python/foxglove-sdk run pytest

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
	cargo test -p foxglove --all-features
	cargo test -p foxglove_c --all-features
	cargo test -p foxglove_data_loader
	cargo test -p foxglove_derive
	cargo test -p foxglove-sdk-python --features remote-access

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
	make -C cpp SANITIZE=address,undefined FOXGLOVE_REMOTE_ACCESS=OFF test

# Build the C/C++ SDK into a directory suitable for use as
# FETCHCONTENT_SOURCE_DIR_FOXGLOVE_SDK in CMake.
CPP_SDK_DIR ?= cpp/dist
FOXGLOVE_REMOTE_ACCESS ?= ON
STATICLIB_NAME ?= libfoxglove.a
CDYLIB_NAME ?= libfoxglove.so
CARGO_LIB_DIR = target/$(if $(CARGO_BUILD_TARGET),$(CARGO_BUILD_TARGET)/)release
.PHONY: build-cpp-dist
build-cpp-dist:
	cd c && FOXGLOVE_SDK_LANGUAGE=c cargo rustc --release --lib --crate-type staticlib
	cd c && FOXGLOVE_SDK_LANGUAGE=c cargo rustc --release --lib --crate-type cdylib \
		$(if $(filter ON,$(FOXGLOVE_REMOTE_ACCESS)),--features remote-access)
	mkdir -p $(CPP_SDK_DIR)/lib $(CPP_SDK_DIR)/include $(CPP_SDK_DIR)/src
	cp $(CARGO_LIB_DIR)/$(STATICLIB_NAME) $(CPP_SDK_DIR)/lib/
	cp $(CARGO_LIB_DIR)/$(CDYLIB_NAME) $(CPP_SDK_DIR)/lib/
	if [ -f "$(CARGO_LIB_DIR)/$(CDYLIB_NAME).lib" ]; then \
		cp "$(CARGO_LIB_DIR)/$(CDYLIB_NAME).lib" "$(CPP_SDK_DIR)/lib/"; \
	fi
	cp -R c/include/foxglove-c $(CPP_SDK_DIR)/include/
	cp -R cpp/foxglove/include/foxglove $(CPP_SDK_DIR)/include/
	cp -R cpp/foxglove/src/* $(CPP_SDK_DIR)/src/

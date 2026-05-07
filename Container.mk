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
	make -C cpp build-clang-tidy

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

# Build the C/C++ SDK as a CMake-installable package suitable for use via find_package or
# FetchContent + add_subdirectory. Two cargo invocations stage both Rust C lib flavors
# (a non-RA staticlib and a cdylib that may or may not have RA, per FOXGLOVE_REMOTE_ACCESS)
# under cpp/build-dist/prebuilt; cmake then builds both C++ wrapper flavors against them
# and installs everything (libs, headers, and the foxglove-sdk CMake package files) into
# CPP_SDK_DIR.
CPP_SDK_DIR ?= cpp/dist
FOXGLOVE_REMOTE_ACCESS ?= ON
STATICLIB_NAME ?= libfoxglove.a
CDYLIB_NAME ?= libfoxglove.so
CARGO_LIB_DIR = target/$(if $(CARGO_BUILD_TARGET),$(CARGO_BUILD_TARGET)/)release
DIST_BUILD_DIR = cpp/build-dist
DIST_PREBUILT_DIR = $(DIST_BUILD_DIR)/prebuilt
.PHONY: build-cpp-dist
build-cpp-dist:
	# Build the staticlib (always non-RA) and cdylib (RA per FOXGLOVE_REMOTE_ACCESS).
	cd c && FOXGLOVE_SDK_LANGUAGE=c cargo rustc --release --lib --crate-type staticlib
	cd c && FOXGLOVE_SDK_LANGUAGE=c cargo rustc --release --lib --crate-type cdylib \
		$(if $(filter ON,$(FOXGLOVE_REMOTE_ACCESS)),--features remote-access)
	# Stage both libs (plus the Windows import lib if present) for cmake's prebuilt path.
	rm -rf $(DIST_BUILD_DIR) $(CPP_SDK_DIR)
	mkdir -p $(DIST_PREBUILT_DIR)
	cp $(CARGO_LIB_DIR)/$(STATICLIB_NAME) $(DIST_PREBUILT_DIR)/
	cp $(CARGO_LIB_DIR)/$(CDYLIB_NAME) $(DIST_PREBUILT_DIR)/
	if [ -f "$(CARGO_LIB_DIR)/$(CDYLIB_NAME).lib" ]; then \
		cp "$(CARGO_LIB_DIR)/$(CDYLIB_NAME).lib" "$(DIST_PREBUILT_DIR)/"; \
	fi
	# Configure + build + install the C++ wrappers and CMake package files.
	cmake -S cpp -B $(DIST_BUILD_DIR) \
		-DCMAKE_BUILD_TYPE=Release \
		-DFOXGLOVE_REMOTE_ACCESS=$(FOXGLOVE_REMOTE_ACCESS) \
		-DFOXGLOVE_BUILD_EXAMPLES=OFF \
		-DFOXGLOVE_PREBUILT_LIB_DIR=$(abspath $(DIST_PREBUILT_DIR)) \
		-DCMAKE_INSTALL_PREFIX=$(abspath $(CPP_SDK_DIR)) \
		-DCMAKE_INSTALL_LIBDIR=lib
	cmake --build $(DIST_BUILD_DIR) --config Release -j $(or $(PARALLEL_JOBS),8)
	cmake --install $(DIST_BUILD_DIR)

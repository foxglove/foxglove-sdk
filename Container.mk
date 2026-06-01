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
# FETCHCONTENT_SOURCE_DIR_FOXGLOVE_SDK in CMake.
CPP_SDK_DIR ?= cpp/dist
FOXGLOVE_REMOTE_ACCESS ?= ON
# Opts into a build-time check that NVENC hardware acceleration for video
# encoding will be available (cuda.h is present on supported targets). Only
# meaningful when remote-access is also enabled. Defaults to OFF because the
# check fails the build on hosts without the CUDA toolkit; opt in explicitly
# (e.g. in CI) where you want the loud failure.
FOXGLOVE_REQUIRE_CUDA ?= OFF
# Selects the rustls crypto backend for the C SDK. Either `aws-lc-rs` (default)
# or `ring`. Override to `ring` on targets where building aws-lc-sys is painful
# (e.g. `aarch64-apple-ios-sim`, which would otherwise need an external bindgen).
FOXGLOVE_CRYPTO_PROVIDER ?= aws-lc-rs
STATICLIB_NAME ?= libfoxglove.a
CDYLIB_NAME ?= libfoxglove.so
CARGO_LIB_DIR = target/$(if $(CARGO_BUILD_TARGET),$(CARGO_BUILD_TARGET)/)release
.PHONY: build-cpp-dist
build-cpp-dist:
	cd c && FOXGLOVE_SDK_LANGUAGE=c cargo rustc --release --lib --crate-type staticlib \
		--no-default-features --features $(FOXGLOVE_CRYPTO_PROVIDER)
	cd c && FOXGLOVE_SDK_LANGUAGE=c cargo rustc --release --lib --crate-type cdylib \
		--no-default-features --features $(FOXGLOVE_CRYPTO_PROVIDER) \
		$(if $(filter ON,$(FOXGLOVE_REMOTE_ACCESS)),--features remote-access) \
		$(if $(filter ON,$(FOXGLOVE_REQUIRE_CUDA)),--features require-cuda)
	mkdir -p $(CPP_SDK_DIR)/lib $(CPP_SDK_DIR)/include $(CPP_SDK_DIR)/src $(CPP_SDK_DIR)/lib/cmake/foxglove-sdk
	cp $(CARGO_LIB_DIR)/$(STATICLIB_NAME) $(CPP_SDK_DIR)/lib/
	cp $(CARGO_LIB_DIR)/$(CDYLIB_NAME) $(CPP_SDK_DIR)/lib/
	if [ -f "$(CARGO_LIB_DIR)/$(CDYLIB_NAME).lib" ]; then \
		cp "$(CARGO_LIB_DIR)/$(CDYLIB_NAME).lib" "$(CPP_SDK_DIR)/lib/"; \
	fi
	cp -R c/include/foxglove-c $(CPP_SDK_DIR)/include/
	cp -R cpp/foxglove/include/foxglove $(CPP_SDK_DIR)/include/
	cp -R cpp/foxglove/src/* $(CPP_SDK_DIR)/src/
	# CMake glue. The config file is hand-written (not generated by cmake) and shipped
	# verbatim so consumers can `find_package(foxglove-sdk CONFIG REQUIRED HINTS <dist>)`.
	# The config includes three shared cmake snippets from the same directory; the
	# local-install path (cpp/CMakeLists.txt) installs the same snippets next to its
	# own config.
	cp cpp/cmake/foxglove-sdk-dist-config.cmake $(CPP_SDK_DIR)/lib/cmake/foxglove-sdk/foxglove-sdkConfig.cmake
	cp cpp/cmake/foxglove-c-imports.cmake $(CPP_SDK_DIR)/lib/cmake/foxglove-sdk/
	cp cpp/cmake/foxglove-static-platform-links.cmake $(CPP_SDK_DIR)/lib/cmake/foxglove-sdk/
	cp cpp/cmake/foxglove-sources.cmake $(CPP_SDK_DIR)/lib/cmake/foxglove-sdk/
	# Record the actual cdylib flavor so the cmake config can validate consumer
	# requests against what's shipped. foxglove_sdk_add_cpp_library(REMOTE_ACCESS ON)
	# errors out if this says OFF.
	echo "set(FOXGLOVE_SDK_CDYLIB_REMOTE_ACCESS $(if $(filter ON,$(FOXGLOVE_REMOTE_ACCESS)),ON,OFF))" \
		> $(CPP_SDK_DIR)/lib/cmake/foxglove-sdk/dist-flavor.cmake

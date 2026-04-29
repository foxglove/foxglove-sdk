.PHONY: all
all: build

BUILD_DIR=build
ifdef CLANG_TIDY
BUILD_DIR=build-tidy
endif
ifdef SANITIZE
comma:=,
BUILD_DIR=build-sanitize-$(subst $(comma),-,$(SANITIZE))
$(info Using sanitizer: $(SANITIZE))
endif

CMAKE_BUILD_TYPE?=RelWithDebInfo
FOXGLOVE_REMOTE_ACCESS?=ON

.PHONY: lint
lint:
	find . -path './build*' -prune -name '*.h' -o -name '*.hpp' -o -name '*.cpp' -o -name '*.inl' |\
		grep -v 'foxglove/expected.hpp' |\
		grep -v '/_deps/' |\
		xargs clang-format --dry-run -Werror --color=1

.PHONY: lint-fix
lint-fix:
	find . -path './build*' -prune -name '*.h' -o -name '*.hpp' -o -name '*.cpp' -o -name '*.inl' |\
		grep -v 'foxglove/expected.hpp' |\
		grep -v '/_deps/' |\
		xargs clang-format -i -Werror --color=1

.PHONY: clean
clean:
	rm -rf $(BUILD_DIR)

.PHONY: build
build:
	cmake \
		-DCMAKE_BUILD_TYPE=$(CMAKE_BUILD_TYPE) \
		-DCMAKE_EXPORT_COMPILE_COMMANDS=true \
		$(if $(SANITIZE),-DSANITIZE=$(SANITIZE) -DRust_TOOLCHAIN=nightly) \
		$(if $(STRICT),-DSTRICT=$(STRICT)) \
		$(if $(CLANG_TIDY),-DCLANG_TIDY=$(CLANG_TIDY)) \
		$(if $(BUILD_OPENCV_EXAMPLE),-DBUILD_OPENCV_EXAMPLE=$(BUILD_OPENCV_EXAMPLE)) \
		$(if $(FOXGLOVE_PREBUILT_LIB_DIR),-DFOXGLOVE_PREBUILT_LIB_DIR=$(FOXGLOVE_PREBUILT_LIB_DIR)) \
		-DFOXGLOVE_REMOTE_ACCESS=$(FOXGLOVE_REMOTE_ACCESS) \
		$(CMAKE_ARGS) \
		-B $(BUILD_DIR)
	cmake --build $(BUILD_DIR) -j $(or $(PARALLEL_JOBS),8) --config $(CMAKE_BUILD_TYPE)

RA_BUILD_DIR=$(BUILD_DIR)-ra

.PHONY: build-ra
build-ra:
	cmake \
		-DCMAKE_BUILD_TYPE=$(CMAKE_BUILD_TYPE) \
		-DCMAKE_EXPORT_COMPILE_COMMANDS=true \
		-DFOXGLOVE_REMOTE_ACCESS=ON \
		$(if $(SANITIZE),-DSANITIZE=$(SANITIZE) -DRust_TOOLCHAIN=nightly) \
		$(if $(STRICT),-DSTRICT=$(STRICT)) \
		$(if $(CLANG_TIDY),-DCLANG_TIDY=$(CLANG_TIDY)) \
		$(if $(FOXGLOVE_PREBUILT_LIB_DIR),-DFOXGLOVE_PREBUILT_LIB_DIR=$(FOXGLOVE_PREBUILT_LIB_DIR)) \
		$(CMAKE_ARGS) \
		-B $(RA_BUILD_DIR)
	cmake --build $(RA_BUILD_DIR) -j $(or $(PARALLEL_JOBS),8) --config $(CMAKE_BUILD_TYPE)

.PHONY: test-ra
test-ra: build-ra
	cd $(RA_BUILD_DIR) && ctest --verbose

INTEGRATION_BUILD_DIR=$(BUILD_DIR)-integration

.PHONY: build-integration
build-integration:
	cmake \
		-DCMAKE_BUILD_TYPE=$(CMAKE_BUILD_TYPE) \
		-DCMAKE_EXPORT_COMPILE_COMMANDS=true \
		-DFOXGLOVE_REMOTE_ACCESS=ON \
		-DFOXGLOVE_BUILD_INTEGRATION_TESTS=ON \
		$(if $(SANITIZE),-DSANITIZE=$(SANITIZE) -DRust_TOOLCHAIN=nightly) \
		$(if $(STRICT),-DSTRICT=$(STRICT)) \
		$(if $(FOXGLOVE_PREBUILT_LIB_DIR),-DFOXGLOVE_PREBUILT_LIB_DIR=$(FOXGLOVE_PREBUILT_LIB_DIR)) \
		$(CMAKE_ARGS) \
		-B $(INTEGRATION_BUILD_DIR)
	cmake --build $(INTEGRATION_BUILD_DIR) -j $(or $(PARALLEL_JOBS),8) --config $(CMAKE_BUILD_TYPE)

.PHONY: test-integration
test-integration: build-integration
	cd $(INTEGRATION_BUILD_DIR) && ctest --verbose -L integration

.PHONY: clean-ra
clean-ra:
	rm -rf $(RA_BUILD_DIR)

.PHONY: clean-integration
clean-integration:
	rm -rf $(INTEGRATION_BUILD_DIR)


.PHONY: test
test: build
	cd $(BUILD_DIR) && ctest --verbose

.PHONY: docs
docs: $(BUILD_DIR)/docs/html

.PHONY: clean-docs
clean-docs:
	rm -rf ./foxglove/docs/generated
	rm -rf $(BUILD_DIR)/docs

$(BUILD_DIR)/docs/html: clean-docs
	cmake -B $(BUILD_DIR)
	cmake --build $(BUILD_DIR) --target doxygen
	cmake --build $(BUILD_DIR) --target html-docs

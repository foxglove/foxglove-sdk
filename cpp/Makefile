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

.PHONY: lint
lint:
	find . -path './build*' -prune -name '*.h' -o -name '*.hpp' -o -name '*.cpp' -o -name '*.inl' |\
		grep -v 'foxglove/expected.hpp' |\
		xargs clang-format --dry-run -Werror --color=1

.PHONY: lint-fix
lint-fix:
	find . -path './build*' -prune -name '*.h' -o -name '*.hpp' -o -name '*.cpp' -o -name '*.inl' |\
		grep -v 'foxglove/expected.hpp' |\
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
		-B $(BUILD_DIR)
	cmake --build $(BUILD_DIR) -j 8 --config $(CMAKE_BUILD_TYPE)

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

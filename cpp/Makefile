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

.PHONY: lint
lint:
	find . -path './build*' -prune -name '*.h' -o -name '*.hpp' -o -name '*.cpp' |\
		grep -v 'foxglove/expected.hpp' |\
		xargs clang-format --dry-run -Werror --color=1

.PHONY: lint-fix
lint-fix:
	find . -path './build*' -prune -name '*.h' -o -name '*.hpp' -o -name '*.cpp' |\
		grep -v 'foxglove/expected.hpp' |\
		xargs clang-format -i -Werror --color=1

.PHONY: clean
clean:
	rm -rf $(BUILD_DIR)

.PHONY: build
build:
	cmake \
		-DCMAKE_C_COMPILER=clang \
		-DCMAKE_CXX_COMPILER=clang++ \
		-DCMAKE_BUILD_TYPE=RelWithDebInfo \
		-DCMAKE_EXPORT_COMPILE_COMMANDS=true \
		$(if $(SANITIZE),-DSANITIZE=$(SANITIZE) -DRust_TOOLCHAIN=nightly) \
		$(if $(STRICT),-DSTRICT=$(STRICT)) \
		$(if $(CLANG_TIDY),-DCLANG_TIDY=$(CLANG_TIDY)) \
		-B $(BUILD_DIR)
	cmake --build $(BUILD_DIR) -j 8 --config RelWithDebInfo

.PHONY: test
test: build
	cd $(BUILD_DIR) && ctest --verbose

.PHONY: docs
docs: $(BUILD_DIR)/docs/html

.PHONY: clean-docs
clean-docs:
	rm -rf ./foxglove/docs/generated
	rm -rf $(BUILD_DIR)/docs

$(BUILD_DIR)/docs/html:
	cmake -B $(BUILD_DIR)
	cmake --build $(BUILD_DIR) --target doxygen
	cmake --build $(BUILD_DIR) --target html-docs

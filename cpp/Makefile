.PHONY: all
all: build

BUILD_DIR=build
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

.PHONY: clean
clean:
	rm -rf $(BUILD_DIR)

.PHONY: build
build:
	cmake \
		-DCMAKE_BUILD_TYPE=RelWithDebInfo \
		$(if $(SANITIZE),-DSANITIZE=$(SANITIZE)) \
		$(if $(CLANG_TIDY),-DCLANG_TIDY=$(CLANG_TIDY)) \
		-B $(BUILD_DIR)
	cmake --build $(BUILD_DIR) -j 8 --config RelWithDebInfo

.PHONY: test
test: build
	cd $(BUILD_DIR) && ctest --verbose

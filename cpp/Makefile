.PHONY: all
all: build

.PHONY: lint
lint:
	find . -path ./build -prune -name '*.h' -o -name '*.hpp' -o -name '*.cpp' | xargs clang-format --dry-run -Werror --color=1

.PHONY: clean
clean:
	rm -rf build

.PHONY: build
build:
	cmake -DCMAKE_BUILD_TYPE=RelWithDebInfo -B build
	cmake --build build -j 8 --config RelWithDebInfo

.PHONY: test
test: build
	cd build && ctest

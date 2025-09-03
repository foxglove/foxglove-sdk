ifndef WASI_SDK
$(error Download the WASI SDK from https://github.com/WebAssembly/wasi-sdk/releases and set WASI_SDK to the extracted path)
endif
SDK_VERSION?=devel

.PHONY: all
all: package example

.PHONY: package
package: target/foxglove_data_loader-$(SDK_VERSION)-cpp-wasm32-unknown-unknown.zip

.PHONY: clean
clean:
	rm -rf target

.PHONY: example
example: target/data_loader_example.wasm

packagefiles:=\
  target/sdk/include/foxglove-c/foxglove-c.h \
  target/sdk/include/foxglove/arena.hpp \
  target/sdk/include/foxglove/schema.hpp \
  target/sdk/include/foxglove/schemas.hpp \
  target/sdk/include/foxglove/expected.hpp \
  target/sdk/include/foxglove/error.hpp \
  target/sdk/include/foxglove_data_loader/data_loader.hpp \
  target/sdk/include/foxglove_data_loader/data_loader.inl \
  target/sdk/include/foxglove_data_loader/host_internal.h \
  target/sdk/include/foxglove_data_loader/host_internal.inl \
  target/sdk/src/foxglove/schemas.cpp \
  target/sdk/src/foxglove/error.cpp \
  target/sdk/lib/libfoxglove.a \
  target/sdk/README.md

target/sdk/include/foxglove-c/%: ../../c/include/foxglove-c/%
	mkdir -p target/sdk/include/foxglove-c
	cp $< $@

target/sdk/include/foxglove_data_loader/%: include/foxglove_data_loader/%
	mkdir -p target/sdk/include/foxglove_data_loader
	cp $< $@

target/sdk/include/foxglove/%: ../foxglove/include/foxglove/%
	mkdir -p target/sdk/include/foxglove
	cp $< $@

target/sdk/src/foxglove/%: ../foxglove/src/%
	mkdir -p target/sdk/src/foxglove
	cp $< $@

target/sdk/lib/libfoxglove.a: cargo-build-foxglove-c
	mkdir -p target/sdk/lib
	cp ../../target/wasm32-unknown-unknown/release/libfoxglove.a $@

target/sdk/README.md: README.md
	cp $< $@

.PHONY: cargo-build-foxglove-c
cargo-build-foxglove-c:
	cd ../../c && cargo build --release --target wasm32-unknown-unknown

target/foxglove_data_loader-$(SDK_VERSION)-cpp-wasm32-unknown-unknown.zip: $(packagefiles)
	cd target/sdk && zip -r ../../$@ .

target/data_loader_example.wasm: examples/data_loader.cpp $(packagefiles)
	$(WASI_SDK)/bin/clang++ \
		$< \
		target/sdk/src/foxglove/schemas.cpp \
		target/sdk/src/foxglove/error.cpp \
		-Itarget/sdk/include \
		-Ltarget/sdk/lib \
		-lfoxglove \
		-o $@ \
		-Wall -Werror \
		-mexec-model=reactor \
		-fno-exceptions \
		--target=wasm32-wasip1 \
		--sysroot=$(WASI_SDK)/share/wasi-sysroot

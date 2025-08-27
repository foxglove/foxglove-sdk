ifndef WASI_SDK
$(error Download the WASI SDK from https://github.com/WebAssembly/wasi-sdk/releases and set WASI_SDK to the extracted path)
endif
SDK_VERSION?=devel

.PHONY: all
all: package example

.PHONY: package
package: foxglove_data_loader-$(SDK_VERSION)-cpp-wasm32-unknown-unknown.zip

.PHONY: clean
clean:
	rm -rf target

.PHONY: example
example: target/data_loader_example.wasm

packagefiles:=\
  target/include/foxglove-c/foxglove-c.h \
  target/include/foxglove/arena.hpp \
  target/include/foxglove/schema.hpp \
  target/include/foxglove/schemas.hpp \
  target/include/foxglove/expected.hpp \
  target/include/foxglove/error.hpp \
  target/include/foxglove_data_loader/data_loader.hpp \
  target/include/foxglove_data_loader/data_loader.inl \
  target/include/foxglove_data_loader/host_internal.h \
  target/include/foxglove_data_loader/host_internal.inl \
  target/src/foxglove/schemas.cpp \
  target/src/foxglove/error.cpp \
  target/lib/libfoxglove.a \
  target/README.md

target/include/foxglove-c/%: ../../c/include/foxglove-c/%
	mkdir -p target/include/foxglove-c
	cp $< $@

target/include/foxglove_data_loader/%: include/foxglove_data_loader/%
	mkdir -p target/include/foxglove_data_loader
	cp $< $@

target/include/foxglove/%: ../foxglove/include/foxglove/%
	mkdir -p target/include/foxglove
	cp $< $@

target/src/foxglove/%: ../foxglove/src/%
	mkdir -p target/src/foxglove
	cp $< $@

target/lib/libfoxglove.a: cargo-build-foxglove-c
	mkdir -p target/lib
	cp ../../target/wasm32-unknown-unknown/release/libfoxglove.a $@

target/README.md: README.md
	cp $< $@

.PHONY: cargo-build-foxglove-c
cargo-build-foxglove-c:
	cd ../../c && cargo build --release --target wasm32-unknown-unknown

foxglove_data_loader-$(SDK_VERSION)-cpp-wasm32-unknown-unknown.zip: $(packagefiles)
	cd target && zip -r ../$@ .

target/data_loader_example.wasm: examples/data_loader.cpp $(packagefiles)
	$(WASI_SDK)/bin/clang++ \
		$< \
		target/src/foxglove/schemas.cpp \
		target/src/foxglove/error.cpp \
		-Itarget/include \
		-Ltarget/lib \
		-lfoxglove \
		-o $@ \
		-Wall -Werror \
		-mexec-model=reactor \
		-fno-exceptions \
		--target=wasm32-wasi

ifndef WASI_SDK
$(error Download the WASI SDK from https://github.com/WebAssembly/wasi-sdk/releases and set WASI_SDK to the extracted path)
endif

.PHONY: all
all: package example

.PHONY: package
package: target/foxglove_data_loader.tar.gz

.PHONY: clean
clean:
	rm -rf target

.PHONY: example
example: target/data_loader_example.wasm

tgzfiles:=\
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
  target/src/schemas.cpp \
  target/lib/libfoxglove.a \
  target/README.md \

target/include/foxglove-c/foxglove-c.h: ../../c/include/foxglove-c/foxglove-c.h
	mkdir -p target/include/foxglove-c
	cp $< $@

target/include/foxglove_data_loader/%: include/foxglove_data_loader/%
	mkdir -p target/include/foxglove_data_loader
	cp $< $@

target/include/foxglove/%: ../foxglove/include/foxglove/%
	mkdir -p target/include/foxglove
	cp $< $@

target/src/schemas.cpp: ../foxglove/src/schemas.cpp
	mkdir -p target/src
	cp $< $@

target/lib/libfoxglove.a: cargo-build-foxglove-c
	mkdir -p target/lib
	cp ../../target/wasm32-unknown-unknown/release/libfoxglove.a $@

target/README.md: README.md
	cp $< $@

.PHONY: cargo-build-foxglove-c
cargo-build-foxglove-c:
	cd ../../c && cargo build --release --target wasm32-unknown-unknown

target/foxglove_data_loader.tar.gz: $(tgzfiles)
	tar --strip-components=1 -czf $@ $^

target/data_loader_example.wasm: examples/data_loader.cpp target/foxglove_data_loader.tar.gz
	$(WASI_SDK)/bin/clang++ \
		$< \
		target/src/schemas.cpp \
		-Itarget/include \
		-Ltarget/lib \
		-lfoxglove \
		-o $@ \
		-Wall -Werror \
		-mexec-model=reactor \
		-fno-exceptions \
		--target=wasm32-wasi

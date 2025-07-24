all: target/component.wasm

target/component.wasm: component.cpp bindings/*.c bindings/*.h
	@mkdir -p target
	$(WASI_SDK)/bin/clang++ \
		component.cpp \
		bindings/data_loader.cpp \
		bindings/host.c bindings/host_component_type.o \
		-o target/component.wasm \
		-mexec-model=reactor \
		-fno-exceptions \
		--target=wasm32-wasi

clean:
	rm target/component.wasm

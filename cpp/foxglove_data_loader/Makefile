ifndef WASI_SDK
$(error Download the WASI SDK from https://github.com/WebAssembly/wasi-sdk/releases and set WASI_SDK to the extracted path)
endif

target/data_loader_example.wasm: examples/data_loader.cpp include/foxglove_data_loader/*
	mkdir -p target
	$(WASI_SDK)/bin/clang++ \
		$< \
		-Iinclude \
		-o $@ \
		-Wall -Werror \
		-mexec-model=reactor \
		-fno-exceptions \
		--target=wasm32-wasi

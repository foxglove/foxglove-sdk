IMAGE_NAME=foxglove-sdk
CONTAINER_MAKEFILE=Container.mk
MSRV_RUST_VERSION=1.85.0

.PHONY: default
default: build-rust

.PHONY: image
image:
	docker build --build-arg MSRV_RUST_VERSION=$(MSRV_RUST_VERSION) \
		-t $(IMAGE_NAME) .

.PHONY: shell
shell: image
	docker run -v $(shell pwd):/app \
		-e CARGO_HOME=/app/.cargo \
		-e UV_CACHE_DIR=/app/.uv_cache \
		-it $(IMAGE_NAME) \
		bash

TARGETS := $(shell awk '/^\.PHONY:/ {for(i=2;i<=NF;i++) print $$i}' $(CONTAINER_MAKEFILE))

.PHONY: $(TARGETS)
$(TARGETS): image
	docker run -v $(shell pwd):/app \
		-e CARGO_HOME=/app/.cargo \
		-e UV_CACHE_DIR=/app/.uv_cache \
		-t $(IMAGE_NAME) \
		make -f $(CONTAINER_MAKEFILE) \
		MSRV_RUST_VERSION=$(MSRV_RUST_VERSION) \
		$@

.PHONY: list-targets
list-targets:
	@echo $(TARGETS) | tr ' ' '\n' | sort

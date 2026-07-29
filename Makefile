IMAGE_NAME=foxglove-sdk
CONTAINER_MAKEFILE=Container.mk
MSRV_RUST_VERSION=1.88.0
# Match Dockerfile: doxygen/flatc are x86_64-only release binaries.
DOCKER_PLATFORM=linux/amd64

.PHONY: default
default: build-rust

.PHONY: image
image:
	docker build --platform $(DOCKER_PLATFORM) \
		--build-arg MSRV_RUST_VERSION=$(MSRV_RUST_VERSION) \
		-t $(IMAGE_NAME) .

.PHONY: shell
shell: image
	docker run --platform $(DOCKER_PLATFORM) -v $(shell pwd):/app \
		-e CARGO_HOME=/app/.cargo \
		-e UV_CACHE_DIR=/app/.uv_cache \
		-it $(IMAGE_NAME) \
		bash

TARGETS := $(shell awk '/^\.PHONY:/ {for(i=2;i<=NF;i++) print $$i}' $(CONTAINER_MAKEFILE))

.PHONY: $(TARGETS)
$(TARGETS): image
	docker run --platform $(DOCKER_PLATFORM) -v $(shell pwd):/app \
		-e CARGO_HOME=/app/.cargo \
		-e UV_CACHE_DIR=/app/.uv_cache \
		-e PYTHON_REMOTE_ACCESS \
		-t $(IMAGE_NAME) \
		make -f $(CONTAINER_MAKEFILE) \
		MSRV_RUST_VERSION=$(MSRV_RUST_VERSION) \
		$@

.PHONY: list-targets
list-targets:
	@echo $(TARGETS) | tr ' ' '\n' | sort

IMAGE_NAME=foxglove-sdk
STABLE_RUST_VERSION=1.83.0
CONTAINER_MAKEFILE=Container.mk

.PHONY: default
default: build-rust

.PHONY: image
image:
	docker build -t $(IMAGE_NAME) .

.PHONY: shell
shell: image
	docker run -v $(shell pwd):/app -it $(IMAGE_NAME) bash

TARGETS := $(shell awk '/^\.PHONY:/ {for(i=2;i<=NF;i++) print $$i}' $(CONTAINER_MAKEFILE))

.PHONY: $(TARGETS)
$(TARGETS): image
	docker run -v $(shell pwd):/app \
		-e CARGO_HOME=/app/.cargo \
		-e POETRY_VIRTUALENVS_PATH=/app/.virtualenvs \
		-it $(IMAGE_NAME) \
		make -f $(CONTAINER_MAKEFILE) $@

.PHONY: list-targets
list-targets:
	@echo $(TARGETS)

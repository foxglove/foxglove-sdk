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

.PHONY: %
%:
	docker run -v $(shell pwd):/app -it $(IMAGE_NAME) make -f $(CONTAINER_MAKEFILE) $@

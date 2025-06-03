IMAGE_NAME=foxglove-sdk
STABLE_RUST_VERSION=1.83.0

.PHONY: default
default: build

.PHONY: image
image:
	docker build -t $(IMAGE_NAME) .

.PHONY: shell
shell: image
	docker run -v $(shell pwd):/app -it $(IMAGE_NAME) bash

.PHONY: build-rust
build-rust: image
	docker run -v $(shell pwd):/app $(IMAGE_NAME) make build-rust-inside

.PHONY: generate
generate: image
	docker run -v $(shell pwd):/app $(IMAGE_NAME) make generate-inside

.PHONY: generate-inside
generate-inside:
	poetry install
	yarn generate

.PHONY: build-rust-inside
build-rust-inside:
	cargo run --bin foxglove_proto_gen && git diff --exit-code
	cargo fmt --all --check
	cargo build --verbose
	#set -euo pipefail
	cargo metadata --no-deps --format-version 1 \
		| jq -r ".packages[].name" \
		| grep example \
		| while read package; do \
			echo "Building $$package" && \
			cargo build -p "$$package"; \
		done
	cargo build -p foxglove --verbose --no-default-features
	# Validate that we can build against the MSRV (minimum specified rust version).
	cargo +$(STABLE_RUST_VERSION) build -p foxglove --verbose
	cargo clippy --no-deps --all-targets --tests -- -D warnings
	cargo +nightly rustdoc -p foxglove --all-features -- -D warnings --cfg docsrs
	cargo test --all-features --verbose
	cargo test -p foxglove --no-default-features --verbose

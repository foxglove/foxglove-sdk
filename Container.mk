.PHONY: generate
generate:
	poetry install
	yarn generate

.PHONY: build-rust
build-rust:
	cargo build --all-targets

.PHONY: lint-rust
lint-rust:
	cargo fmt --all --check
	cargo clippy --no-deps --all-targets --tests -- -D warnings

.PHONY: test-rust
test-rust:
	cargo test --all-features

.PHONY: test-rust-foxglove-no-default-features
test-rust-foxglove-no-default-features:
	cargo test -p foxglove --no-default-features

.PHONY: rust-docs
rust-docs:
	cargo +nightly rustdoc -p foxglove --all-features -- -D warnings --cfg docsrs

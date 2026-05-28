.PHONY: all
all: protoc lint

.PHONY: protoc
protoc:
	uv run protoc --python_out=. --pyi_out=. fruit.proto

.PHONY: lint
lint:
	uv run black .
	uv run isort .

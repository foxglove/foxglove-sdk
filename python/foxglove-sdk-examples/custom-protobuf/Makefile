.PHONY: all
all: protoc lint

.PHONY: protoc
protoc:
	poetry run protoc --python_out=. --pyi_out=. fruit.proto

.PHONY: lint
lint:
	poetry run black .
	poetry run isort .

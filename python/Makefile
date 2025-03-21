ifeq ($(shell uname),Darwin)
SED_ARGS = -i ''
else
SED_ARGS = -i
endif

.PHONY: all
all: clean install generate build test

.PHONY: install
install:
	cd foxglove-schemas-flatbuffer && poetry install
	cd foxglove-schemas-protobuf && poetry install

.PHONY: generate-flatbuffer
generate-flatbuffer:
	find foxglove-schemas-flatbuffer/foxglove_schemas_flatbuffer ! -name '__init__.py' -type f -exec rm -f {} +
	cd foxglove-schemas-flatbuffer && poetry run flatc \
		--require-explicit-ids \
		--python \
		-o foxglove_schemas_flatbuffer \
		../../schemas/flatbuffer/*.fbs
	cd foxglove-schemas-flatbuffer && poetry run flatc \
		-b \
		--schema \
		-o foxglove_schemas_flatbuffer \
		../../schemas/flatbuffer/*.fbs
	rm foxglove-schemas-flatbuffer/foxglove_schemas_flatbuffer/foxglove/__init__.py
	mv foxglove-schemas-flatbuffer/foxglove_schemas_flatbuffer/foxglove/* foxglove-schemas-flatbuffer/foxglove_schemas_flatbuffer
	rmdir foxglove-schemas-flatbuffer/foxglove_schemas_flatbuffer/foxglove
	sed -E $(SED_ARGS) 's/from foxglove\./from \./g' foxglove-schemas-flatbuffer/foxglove_schemas_flatbuffer/*.py

.PHONY: generate-protobuf
generate-protobuf:
	rm -rf foxglove-schemas-protobuf/foxglove_schemas_protobuf/*_pb2*
	cd foxglove-schemas-protobuf && poetry run protoc \
		--python_out=foxglove_schemas_protobuf \
		--mypy_out=foxglove_schemas_protobuf \
		--proto_path ../../schemas/proto \
		../../schemas/proto/foxglove/*.proto
	mv foxglove-schemas-protobuf/foxglove_schemas_protobuf/foxglove/* foxglove-schemas-protobuf/foxglove_schemas_protobuf
	rmdir foxglove-schemas-protobuf/foxglove_schemas_protobuf/foxglove
	sed -E $(SED_ARGS) 's/from foxglove import/from . import/g' foxglove-schemas-protobuf/foxglove_schemas_protobuf/*_pb2.py
	sed -E $(SED_ARGS) 's/import foxglove\.(.+)$$/from . import \1 as foxglove_\1/g' foxglove-schemas-protobuf/foxglove_schemas_protobuf/*_pb2.pyi
	sed -E $(SED_ARGS) 's/foxglove\./foxglove_/g' foxglove-schemas-protobuf/foxglove_schemas_protobuf/*_pb2.pyi

.PHONY: generate
generate: generate-flatbuffer generate-protobuf

.PHONY: build
build: generate
	cd foxglove-schemas-flatbuffer && poetry build
	cd foxglove-schemas-protobuf && poetry build

.PHONY: test
test:
	cd foxglove-schemas-flatbuffer && poetry run pytest
	cd foxglove-schemas-protobuf && poetry run pytest

.PHONY: clean
clean:
	git clean -Xdf

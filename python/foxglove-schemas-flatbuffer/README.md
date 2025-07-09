# Foxglove Schemas (Flatbuffer)

This package provides [Flatbuffer](https://google.github.io/flatbuffers/) classes for [Foxglove Schemas](https://docs.foxglove.dev/docs/visualization/message-schemas/introduction).

## Installation

Install via [Poetry](https://python-poetry.org/):

```sh
poetry add foxglove-schemas-flatbuffer
```

## Usage

Import types from the `foxglove_schemas_flatbuffer` module as follows:

```py
import flatbuffers
import foxglove_schemas_flatbuffer.CompressedImage as CompressedImage
from foxglove_schemas_flatbuffer import get_schema

builder = flatbuffers.Builder(1024)
png = builder.CreateString("png")
CompressedImage.Start(builder)
CompressedImage.AddFormat(builder, png)
img = CompressedImage.End(builder)
builder.Finish(img)

# Serialized flatbuffer schema
schema_data = get_schema("CompressedImage")

# Serialized CompressedImage message
msg_data = builder.Output()
```

## Stay in touch

Join our [Discord](https://foxglove.dev/chat) to ask questions, share feedback, and stay up to date on what our team is working on.

# @foxglove/messages

This package contains TypeScript type definitions and JSON Schemas for Foxglove message types.

## Installation

```bash
npm install @foxglove/messages
```

## Usage

For a list of available message types, see https://docs.foxglove.dev/docs/visualization/message-schemas/introduction

```ts
import type { CompressedImage } from "@foxglove/messages";
import { CompressedImage } from "@foxglove/messages/jsonschema";
```

## Migration from @foxglove/schemas

This package was renamed from `@foxglove/schemas` to `@foxglove/messages` to better reflect that these are message type definitions, not schema format files.

To migrate:

1. Update your package.json dependency from `@foxglove/schemas` to `@foxglove/messages`
2. Update your imports from `@foxglove/schemas` to `@foxglove/messages`

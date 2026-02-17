# @foxglove/messages

Foxglove message type definitions for TypeScript.

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

## Relationship to @foxglove/schemas

This package currently re-exports all types from `@foxglove/schemas`. In a future version, the relationship will be reversed: this package will contain the canonical type definitions and `@foxglove/schemas` will re-export from here for backward compatibility.

You can use either package interchangeably. New projects are encouraged to use `@foxglove/messages`.

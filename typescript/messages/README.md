# @foxglove/messages

This package contains TypeScript type definitions and JSON Schemas for the Foxglove message types.

> **Note:** This package replaces `@foxglove/schemas`. The old package continues to work but re-exports from this one.

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

## Migrating from `@foxglove/schemas`

Replace your dependency:

```diff
- "@foxglove/schemas": "..."
+ "@foxglove/messages": "..."
```

Update your imports:

```diff
- import type { CompressedImage } from "@foxglove/schemas";
+ import type { CompressedImage } from "@foxglove/messages";
```

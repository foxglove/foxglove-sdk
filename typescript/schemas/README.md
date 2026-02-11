# @foxglove/schemas

> **Deprecated**: This package has been renamed to [`@foxglove/messages`](https://www.npmjs.com/package/@foxglove/messages). Please update your dependencies and imports.

This package re-exports everything from `@foxglove/messages` for backward compatibility.

## Migration

1. Update your package.json:

```diff
- "@foxglove/schemas": "^1.9.0"
+ "@foxglove/messages": "^1.9.0"
```

2. Update your imports:

```diff
- import { CompressedImage } from "@foxglove/schemas";
+ import { CompressedImage } from "@foxglove/messages";
```

## Why the rename?

The package was renamed from `@foxglove/schemas` to `@foxglove/messages` to better reflect that these are **message type definitions**, not schema format files. Schema formats (like JSON Schema or Protobuf `.proto` files) remain in the `schemas/` directory of the repository.

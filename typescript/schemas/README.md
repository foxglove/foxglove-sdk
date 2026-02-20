# @foxglove/schemas (deprecated)

> **Deprecated:** This package re-exports from [`@foxglove/messages`](../messages). Please migrate to `@foxglove/messages` for new projects.

## Migration

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

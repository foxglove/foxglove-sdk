# Claude Notes - foxglove-sdk

## Current Work: Schema-to-Message Rename (Rust SDK Step)

### Branch: fle-13-generate

### What was done
Ported the Rust SDK portion of the `fle-13-schema-to-message` branch changes:
- Renamed `schemas` module to `messages` throughout the Rust SDK
- Added deprecated `schemas` re-export module for backward compatibility
- Updated all internal references, examples, tests, and codegen
- Updated TypeScript codegen (`generateSdkRustCTypes.ts`) to emit `foxglove::messages::`
- Updated `scripts/generate.ts` Rust output path

### What was NOT changed (left for future PRs)
- C++ code (`cpp/`) - still uses `foxglove::schemas` namespace
- Python SDK (`python/foxglove-sdk/`) - still uses `foxglove::schemas::` (works via deprecated re-export)
- TypeScript package restructuring (no `@foxglove/messages` package yet)
- `generateSdkCpp.ts` - still generates `foxglove::schemas` namespace
- `generatePyclass.ts` - still generates `foxglove::schemas::` references

### Key decisions
- `since` version set to `"0.18.0"` (next minor after current `0.17.2`)
- Python SDK compiles cleanly via deprecated `schemas` module (no warnings in CI)
- C bindings (`foxglove_c`) updated to use `foxglove::messages::` directly

### Test results
- 238 unit tests pass
- 23 doc-tests pass
- cargo fmt clean
- clippy clean with `-D warnings`
- TypeScript: 197 tests pass, prettier clean

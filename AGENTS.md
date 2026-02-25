---
description:
globs:
alwaysApply: true
---

# Foxglove SDK Codebase Rules

Instructions for AI (and human) contributors.

## About Foxglove

Foxglove is an observability and visualization platform for multimodal robotics data.
This repository is the Foxglove SDK: a multi-language SDK for logging and visualizing multimodal data.
The core SDK is written in Rust, with bindings for Python and C/C++, plus TypeScript schema definitions and ROS packages.

## Foundational Principles

- Preserve existing patterns, naming conventions, and project structure
- Minimize changes - modify only what is necessary for quality, maintainability, and the task at hand

## Repository Layout

| Directory                            | Purpose                                                         |
| ------------------------------------ | --------------------------------------------------------------- |
| `rust/foxglove`                      | Core Rust SDK crate                                             |
| `rust/foxglove_derive`               | Derive macros for the Rust SDK                                  |
| `rust/foxglove_proto_gen`            | Protobuf code generation for Rust                               |
| `rust/foxglove_data_loader`          | Data loader crate (runs in app via WASM)                        |
| `rust/data_provider_conformance`     | Conformance tests for data providers                            |
| `rust/examples/`                     | Rust example programs                                           |
| `c/`                                 | C SDK (FFI layer built on top of Rust)                          |
| `cpp/foxglove`                       | C++ SDK                                                         |
| `cpp/foxglove_data_loader`           | C++ data loader                                                 |
| `cpp/examples/`                      | C++ example programs                                            |
| `python/foxglove-sdk`                | Python SDK (PyO3 bindings to Rust core)                         |
| `python/foxglove-sdk-examples`       | Python example programs                                         |
| `python/foxglove-schemas-flatbuffer` | Flatbuffer schema definitions for Python                        |
| `python/foxglove-schemas-protobuf`   | Protobuf schema definitions for Python                          |
| `typescript/schemas`                 | TypeScript schema definitions                                   |
| `schemas/`                           | Schema definitions (flatbuffer, jsonschema, omgidl, proto, ros) |
| `ros/`                               | ROS message package                                             |
| `scripts/`                           | Build and code generation scripts                               |
| `playground/`                        | Interactive playground/examples                                 |

## Technology Stack

- **Rust** - Core SDK implementation; Cargo workspace at the repo root
- **Python** - PyO3 bindings; managed with `uv`; type-checked with `mypy`; formatted with `black` + `isort`; linted with `flake8`
- **C/C++** - FFI layer (C) and idiomatic wrapper (C++); built with CMake + `cargo` (via `corrosion`)
- **TypeScript** - Schema definitions, codegen, and CI scripts; managed with `yarn`; tested with `jest`
- **Schemas** - Protobuf, Flatbuffers, JSON Schema, OMG IDL, ROS 1/2 message definitions; generated via `make generate`
  - The schemas are defined in `typescript/schemas/src/internal/schemas.ts`

## High-level Architecture

- Context — binds channels to sinks; channels and sinks belong to exactly one context; the global context is used by default
- Channel — typed or untyped message stream on a topic; created once, reused for all messages on that topic
- Sink — receives logged messages; built-in sinks are McapWriter (file logging), WebSocketServer (live visualization), remote access Gateway (remote viz & teleop)
- The Python and C/C++ APIs are thin wrappers over this same model

## Development Guidelines

### Rust

- Prior to committing changes or considering them completed, for the modified rust project(s) run:
  `cargo test --all-features`
  `cargo test --no-default-features`
  `cargo fmt`
  Run cargo check and clippy for the entire workspace:
  `cargo check --all-features`
  `cargo clippy --no-deps --tests --all-features -- -D warnings`
- Prefer `crate::` import over `super::` import; though importing `super::*` is fine within a test module (`mod tests`)
- Use `mod tests` rather than `mod test` for declaring unit tests in a submodule
- The MSRV (Minimum Supported Rust Version) is defined in Cargo.toml. Don't use Rust features that aren't stabilized as of this version.
- Use the tracing crate (tracing::info!, tracing::warn!, etc.), not println!, eprintln!, or the log crate macros directly
- Modules should be defined as `foo.rs`, not `foo/mod.rs`
- Use `cargo public-api` for evaluating public API changes

### Python

- The Python SDK is a maturin/PyO3 project — it has a Rust extension compiled into it. After making any Rust change, you need to reinstall the Python package (`uv pip install --editable .`) for Python tests to reflect the change.
- Use `uv` to manage dependencies and run commands (e.g. `uv run pytest`)
- Format with `black` and sort imports with `isort` (profile `black`) before committing
- Run `flake8` for PEP 8 compliance
- Type-check with `mypy`
- Co-locate `test_*.py` files with source, or place them in a `tests/` directory following existing conventions
- Tests use pytest
- Python code should use modern features and idioms where applicable, the oldest version we need to support is defined in `python/foxglove-sdk/pyproject.toml`

### C / C++

- Build with CMake; the C layer is generated from Rust via `cbindgen`, if the `c/` project is changed, use `cd c; cargo build` to regenerate it.
- The C++ layer depends on the C layer.
- Prior to committing C++ changes or considering them completed run from the repo root:
  `make lint-fix-cpp`
  `make build-cpp-tidy`
  `make test-cpp`
- Tests use Catch2 (v3)
- C++ code should use modern C++17 idioms where applicable, that's the oldest version we need to support

### TypeScript (Schemas only)

- Use `yarn` to install dependencies and run scripts
- Prefer explicit types; avoid `any`

### Schema Generation

- Schema definitions are the source of truth in `scripts/generate.ts`
- After modifying schemas, run `make generate` to regenerate all language-specific outputs
- Generated files are committed to the repository for ease of access

### Utility Scripts

- Scripts in `scripts/` are TypeScript; run them with `ts-node` via `yarn run`

### Testing

- Co-locate tests with source files wherever the language/ecosystem convention supports it
- Use descriptive test names that explain the behavior under test
- Avoid deleting or changing existing test cases unless it is related to the task at hand
- Follow existing patterns in the file (mocking approach, test structure, assertion style) unless explicitly asked to change
- Only include tests that meaningfully exercise production code — avoid tautological or trivial tests

### Code Comments

**Preferred patterns:**

- **Doc comments** (`///` in Rust, `/** ... */` in TypeScript/C++, `"""..."""` in Python): Use for documenting exported functions, types, classes, and complex logic. Include parameter and return descriptions where helpful.
- **Single-line (`// ...` or `# ...`)**: Use for inline explanations, implementation notes, and clarifying non-obvious code.

**Avoid:**

- `TODO` / `FIXME` / `XXX` comments
- Comments reflecting the development process or iteration history (e.g., "Fixed bug where...", "Refactored from...")
- License headers in new files (existing headers for third-party code should remain)
- Redundant comments that restate what the code already expresses
- Comments as section dividers (use named functions or separate files for organization)

### Security & Safety

- Avoid `unsafe` Rust unless strictly necessary; document any `unsafe` block with a safety comment explaining the invariants upheld
- Prefix unused vars/params with `_` in Rust and Python
- Do not reassign parameters
- All code should propagate errors explicitly rather than swallowing them

### Error Handling

- Rust: use `Result<T, E>` and propagate errors with `?`; avoid `.unwrap()` in library code (use `.expect("reason")` or proper error handling)
- Python: raise typed exceptions; avoid bare `except:` clauses
- C/C++: follow existing error-handling conventions in each sub-library

### File & Directory Conventions

- Follow language-idiomatic naming: `snake_case` for Rust, Python, and C/C++ files and identifiers; `camelCase`/`PascalCase` for TypeScript where conventional
- Exported names should mirror filenames where practical
- Avoid unnecessary indirection; prefer direct imports over re-export layers

### Documentation & Quality Gates

- Commit messages must be descriptive
- Write tests for new features and bug fixes
- Run the checks, formatting, and lints for the relevant language(s) specified in this file.
  - **Before pushing**, run applicable tests and auto-fixes on changed files

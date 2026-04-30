# nexus-contracts — Generated Wire Types

Monorepo-internal crate — **not published to crates.io**. All types generated from `schemas/` via `pnpm run codegen`.

## Strict Rules

- **Do NOT hand-edit** any file under `src/generated/` — always regenerate from schemas.
- **Do NOT format generated code with stable rustfmt** — always use `cargo +nightly fmt`.
- **Do NOT add handwritten types** that duplicate generated DTOs. Extensions go in `src/` (outside `generated/`), referencing generated types by re-export.
- After any schema change, run `pnpm run codegen` and commit updated `src/generated/`.

## `enum_conversions.rs`

`src/enum_conversions.rs` is maintained alongside (not by) codegen. When JSON Schema adds/renames enum values, update this file in the same commit and verify with `cargo test -p nexus-contracts`.

See [`schemas/AGENTS.md`](../../schemas/AGENTS.md) for schema-level conventions and codegen flow.

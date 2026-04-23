# nexus-contracts — Generated Wire Types

Monorepo-internal crate — **not published to crates.io**. All types are generated from `schemas/` via `pnpm run codegen`.

## Strict Rules

- **Do NOT hand-edit** any file under `src/generated/` — always regenerate from schemas.
- **Do NOT add handwritten types** that duplicate generated DTOs. If a type is needed beyond what codegen produces, add it in `src/` (outside `generated/`) and reference generated types by re-export.
- After any schema change in `schemas/`, run `pnpm run codegen` and commit the updated `src/generated/` output.

## `enum_conversions.rs`

`src/enum_conversions.rs` is maintained **next to** generated types, not produced by codegen. When JSON Schema adds or renames enum values, update this file in the same commit as regenerated `src/generated/` and verify with `cargo test -p nexus-contracts`.

## Consumers

Both `nexus42` (CLI) and `nexus42d` (daemon) depend on this crate for shared wire types. Downstream crates (`nexus-domain`, `nexus-sync`, `nexus-local-db`, `nexus-orchestration`) also depend on it.

See [`schemas/AGENTS.md`](../../schemas/AGENTS.md) for schema-level conventions and codegen flow.

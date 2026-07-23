# rust-gen — typify Rust Wire-Type Generator

External binary that emits Rust wire types for `crates/nexus-contracts/src/generated/`
from the Nexus JSON Schemas, using [`typify`](https://crates.io/crates/typify).
This is the Rust counterpart to `tooling/codegen/src/ts-gen.ts` (TypeScript).

## Why an external `[workspace]`

This crate declares its own empty `[workspace]`, so it is **NOT** a member of the
repo root `[workspace]`. Consequences:

- Excluded from `cargo build --all` / `cargo clippy --all` / `cargo test --all`.
- `typify` + `schemars` stay out of the main workspace dependency graph (mirrors
  the sibling `spoke` repo's `rust-gen` layout).
- Build/run directly: `cargo run --release` from this directory.

## Input

Consumes the **dereferenced** schema tree produced by stage 1 of the codegen
pipeline (`tooling/codegen/src/schema-prep.ts` → `tooling/codegen/.schemas-dereferenced/`).
Dereferenced schemas are self-contained (no cross-file `$ref`), which `typify`
requires.

## Env contract

| Var | Default | Purpose |
|-----|---------|---------|
| `NEXUS_REPO_ROOT` | `CARGO_MANIFEST_DIR/../../..` | Repo root (output path + sibling defaults) |
| `NEXUS_DEREF_SCHEMAS_DIR` | `<repo>/tooling/codegen/.schemas-dereferenced` | `typify` input tree |
| `NEXUS_SRC_SCHEMAS_DIR` | `<repo>/schemas` | Original schemas (canonical source root logging) |

## Behavior

- Globs `**/*.schema.json` under the deref tree.
- Skips definition-only / canonical-skip schemas: `common/common.schema.json`,
  `common/source-anchor.schema.json`, `platform/sync/bundle-refinement.schema.json`.
- Mirrors the schema tree into `crates/nexus-contracts/src/generated/` with Rust
  module naming (kebab-case → snake_case).
- Emits one `.rs` per schema; fails the run if any schema errors or zero emit.

## Status (plan v1.138)

- **T1 (this crate):** binary skeleton; emits `.rs` for all non-skipped schemas.
- **T2:** barrel `mod.rs` generation (export-mode heuristics read from source schemas).
- **T3:** wire into `tooling/codegen/src/index.ts`; add a `cargo +nightly fmt` pass.
- **T4:** clippy tuning (derives / allows).
- **T5:** reconcile `crates/nexus-contracts/tests/schema_drift_detection.rs`.

Known T1 divergence (expected, to be reconciled in T5): `typify` derives type
names from the schema `title` (e.g. `"Nexus World Entity"` → `NexusWorldEntity`),
whereas the current hand-tuned generator names from the file name (`World`).

See [`../AGENTS.md`](../AGENTS.md) for the codegen pipeline and drift detection.

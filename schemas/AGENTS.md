# Schemas — JSON Schema Wire Contracts

This directory is the **single truth source** for **wire** types (CLI ↔ platform sync and platform HTTP). All Rust and TypeScript types here are generated from JSON Schema.

**Layout (folders):** [`.agents/knowledge/specs/schemas-directory-layout.md`](../.agents/knowledge/specs/schemas-directory-layout.md) — tree index in [README.md](README.md).

**Not in `schemas/`**: local-only types (`/v1/local/*`, orchestration, ACP registry, worker IPC, on-disk/SQLite records) live as hand-written Rust in `crates/nexus-contracts/src/local/`. See [`.agents/knowledge/schemas-wire-platform-sync-boundary.md`](../.agents/knowledge/schemas-wire-platform-sync-boundary.md).

**Cloud line only:** daemon Local API must not add schemas here; sync/register go through `nexus-cloud-sync` per [local-cloud-crate-architecture.md](../.agents/knowledge/specs/local-cloud-crate-architecture.md).

## Schema URI Placeholder

Committed schemas use `https://nexus42.invalid` in `$id`/`$ref` (RFC 6761 reserved; production domain TBD). In prose, use `{NEXUS42_BASE_URL}`. Do **not** embed `{NEXUS42_BASE_URL}` inside JSON `$id`/`$ref` strings.

## Codegen Flow

`schemas/` → `pnpm run codegen` → Rust (`crates/nexus-contracts/src/generated/`) + TypeScript (`packages/nexus-contracts/src/generated/`).

## ⚠️ Mandatory: Run Codegen After Any Schema Change

**Rule:** any commit touching `schemas/` MUST include regenerated output from `pnpm run codegen`. CI (`verify-codegen`) checks `git diff` on generated directories and fails if out of sync. Do NOT hand-edit files under `*/generated/`.

## `enum_conversions.rs` (Manual Companion)

`crates/nexus-contracts/src/enum_conversions.rs` is maintained alongside (not by) codegen. When JSON Schema adds/renames enum values, update this file in the same commit and verify with `cargo test -p nexus-contracts`.

## Wire/Local Schema Drift Detection

CI gate `cargo test --test schema_drift_detection` validates that registered schemas match their Rust struct definitions. Two check modes:

- **`Strict`** (wire types): exact bidirectional property match — every schema property maps to a Rust field and vice versa.
- **`Subset`** (local-only types): only required schema fields enforced; Rust struct may have extra internal fields.

Register new schemas by adding an `entry!` macro to `build_schema_map()` in `crates/nexus-contracts/tests/schema_drift_detection.rs`, then run `./tooling/check-wire-drift.sh`.

## Wire `schema_version`

**`LATEST_SCHEMA_VERSION`: `1`** — constant emitted by codegen into both Rust and TypeScript. Individual DTOs carry per-type `schema_version`; the bundle envelope aligns with the latest value after `pnpm run codegen`.

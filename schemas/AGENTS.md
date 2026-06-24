# Schemas — JSON Schema External-Consumer Contracts

This directory is the **single truth source** for types consumed by an **external client** (cross-language / cross-process boundary): `nexus-platform` (wire) and external Local API clients (e.g. the future WebApp/Web-UI; WASM compute modules). All Rust and TypeScript types here are generated from JSON Schema.

**Layout (folders):** [`.mstar/knowledge/specs/schemas-directory-layout.md`](../.mstar/knowledge/specs/schemas-directory-layout.md) — tree index in [README.md](README.md).

**Not in `schemas/`**: a type belongs here **only if** an external client consumes it. Local-only types (`/v1/local/*` daemon DTOs, orchestration, ACP registry, worker IPC, on-disk/SQLite records) live as hand-written Rust in `crates/nexus-contracts/src/local/`. See [`.mstar/knowledge/schemas-external-consumer-boundary.md`](../.mstar/knowledge/schemas-external-consumer-boundary.md).

**Cloud line only:** daemon internal Local API must not add schemas here; sync/register go through `nexus-cloud-sync` per [local-cloud-crate-architecture.md](../.mstar/knowledge/specs/local-cloud-crate-architecture.md). The `local-api/` subtree is reserved for cross-language Local API contracts (e.g. `local-api/compute/` for the WASM compute ABI).

## Schema URI Placeholder

Committed schemas use `https://nexus42.invalid` in `$id`/`$ref` (RFC 6761 reserved; production domain TBD). In prose, use `{NEXUS42_BASE_URL}`. Do **not** embed `{NEXUS42_BASE_URL}` inside JSON `$id`/`$ref` strings.

## Codegen Flow

`schemas/` → `pnpm run codegen` → Rust (`crates/nexus-contracts/src/generated/`) + TypeScript (`packages/nexus-contracts/src/generated/`).

Generated modules are **nested** to mirror the consumer-scope tree:
- Rust: `generated::{common, domain, platform::{http_bff, sync}, local_api::{compute, works, kb, findings, schedule, workspace, creators}}::<module>` (e.g. `generated::local_api::works::work_summary::WorkSummary`). The root `generated::mod.rs` also re-exports all leaf types flat, so `generated::WorkSummary` resolves too.
- TypeScript: mirrors the same folders (hyphenated: `platform/http-bff`, `local-api/works`, `local-api/kb`, etc.); `index.ts` re-exports flat for the package public API.

The `local-api/` subtree runs through the same codegen as wire types — it is a cross-language contract surface. V1.63 P1 added `local-api/{works,kb,findings,schedule,workspace,creators}/` for the daemon's core CRUD Local API surface; V1.63 P3 added `local-api/{orchestration,preset-management}/` for orchestration sessions/capabilities and preset management. Consumed by future WebApp/Web-UI clients.

## ⚠️ Mandatory: Run Codegen After Any Schema Change

**Rule:** any commit touching `schemas/` MUST include regenerated output from `pnpm run codegen`. CI (`verify-codegen`) checks `git diff` on generated directories and fails if out of sync. Do NOT hand-edit files under `*/generated/`.

## `enum_conversions.rs` (Manual Companion)

`crates/nexus-contracts/src/enum_conversions.rs` is maintained alongside (not by) codegen. When JSON Schema adds/renames enum values, update this file in the same commit and verify with `cargo test -p nexus-contracts`.

## Wire/Local Schema Drift Detection

CI gate `cargo test --test schema_drift_detection` validates that registered schemas match their Rust struct definitions. Two check modes:

- **`Strict`** (external-consumer types): exact bidirectional property match — every schema property maps to a Rust field and vice versa.
- **`Subset`** (local-only types): only required schema fields enforced; Rust struct may have extra internal fields.

Register new schemas by adding an `entry!` macro to `build_schema_map()` in `crates/nexus-contracts/tests/schema_drift_detection.rs` (use the schema's path under `schemas/`), then run `./tooling/check-wire-drift.sh`.

## Wire `schema_version`

**`LATEST_SCHEMA_VERSION`: `1`** — constant emitted by codegen into both Rust and TypeScript. Individual DTOs carry per-type `schema_version`; the bundle envelope aligns with the latest value after `pnpm run codegen`.

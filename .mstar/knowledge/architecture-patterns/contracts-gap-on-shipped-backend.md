---
module: contracts-codegen
date: 2026-07-01
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: 2026-07-01-v1.78-creator-memory-surface
tags: [contracts, codegen, json-schema, dto-normalization, sqlx, orphan-rule, daemon-runtime]
applies_when: [authoring-local-api-surface, normalizing-handwritten-dtos, adding-schemas-to-shipped-handler]
---

# Contracts-Gap on a Shipped Backend Handler

## Context

Nexus enforces a strict wire-contract boundary: `schemas/` JSON Schemas are the single source of truth, `tooling/codegen` generates both TypeScript (`@42ch/nexus-contracts`) and Rust (`crates/nexus-contracts`) types, and consumers (the daemon runtime, the web app) **must not hand-write wire DTOs** (`crates/nexus-daemon-runtime/AGENTS.md`: "Contract types: shares generated types from `crates/nexus-contracts`. Do NOT hand-write duplicate DTOs.").

Despite this invariant, a handler can ship with **hand-written inline DTOs** if it was added before (or bypassed) the schema pipeline. V1.33 shipped the memory Local API (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs`) with 14 hand-written `serde` structs (`CreatePendingReviewRequest`, `PendingReviewInfo`, `ReviewResponse`, `FragmentInfo`, …). The routes worked at runtime, but there were **no** `schemas/local-api/memory/` files and **no** generated types — the OSS contract surface was missing. V1.78 closed this gap.

## Guidance

When you discover (or are asked to consume) a shipped Local API handler whose request/response types are **not** in `schemas/local-api/` + `generated/local-api/`:

1. **Transcribe verbatim, do not redesign.** Read every `#[derive(Deserialize/Serialize)]` struct in the handler and transcribe its fields (name, optionality, type) into net-new JSON Schemas under `schemas/local-api/<domain>/`. Field names, `Option<T>` optionality, and types must mirror runtime **exactly**. Validation caps (max-length etc.) stay handler-owned — they are not a contract redesign.
2. **Let the codegen glob discover them.** The codegen is glob-based (`tooling/codegen/src/schema-loader.ts`: `globSync(schemasDir + '**/*.schema.json')`). No config change, no inclusion list. Run `pnpm run codegen`; the new schemas auto-emit TS + Rust into `generated/local-api/<domain>/`. Bump `@42ch/nexus-contracts` additive (e.g. 0.12.0 → 0.13.0).
3. **Normalize the handler to consume its own generated types.** Replace the hand-written structs with `pub use nexus_contracts::{...}` imports. Behavior must stay identical. Add a Rust round-trip regression test asserting a serialized generated type deserializes to the same handler response shape (e.g. `tests/<domain>_dto_roundtrip.rs`).

### The two non-obvious traps

**Trap A — `sqlx::FromRow` orphan rule.** If a hand-written DTO also served as a SQL row projection (it derived both `Serialize` **and** `sqlx::FromRow`), the generated type **cannot** carry `FromRow`: `sqlx::FromRow` and the generated type are both foreign to the daemon-runtime crate (orphan rule), and `nexus-contracts` intentionally does not depend on sqlx. **Resolution**: switch those `query_as!` sites to `query!` + an explicit field map into the generated type. Do **not** introduce a second hand-written SQL-projection struct — that reintroduces the duplication the normalization set out to remove. Centralize the mapping in one helper used by all fetch sites for that row.

**Trap B — `ambiguous_glob_reexports` at the generated crate root.** The Rust codegen emits a flat `pub use local_api::*; pub use domain::*;` at the crate root. If a new `local_api/<name>/` module collides with an existing `domain/<name>` module (e.g. `local_api::memory` vs `domain::memory`), the flat-glob produces an `ambiguous_glob_reexports` warning at the generated crate root — even though the flat **type** names stay unique (only the module namespace collides). **Resolution**: the generator emits `#![allow(ambiguous_glob_reexports)]` at the generated crate root. This is benign (the ambiguity is module-level, not type-level) and durable (future colliding schemas won't reintroduce the warning).

## Why This Matters

- **Invariant restoration.** Hand-written DTOs are silent contract drift: the runtime works, external consumers can't tell, and a second hand-written DTO appears in the next PR. Normalizing back to generated types restores the single-source-of-truth boundary the whole cross-language stack depends on.
- **External consumability.** Until the schemas exist + are codegen'd + barrel-exported, the `@42ch/nexus-contracts` package doesn't carry the types — no external consumer (the web app, the desktop shell, future MCP server) can consume the surface type-safely. Closing the gap is what makes the surface real.
- **Cheap when caught, expensive when deferred.** The memory gap survived V1.33 → V1.77 (14 iterations) because no consumer needed the types. The moment a UI (or MCP server) needs them, the gap becomes the critical path. Transcribing is mechanical once you read the handler; the cost is in discovering the gap late.

## When to Apply

- Adding a UI / consumer for a backend route whose response type isn't in `@42ch/nexus-contracts`.
- Auditing a crate and finding `#[derive(Serialize)]` structs in a `handlers/*.rs` that duplicate a wire shape.
- Adding the **first** schema for a domain that already has a shipped handler.
- The QC signal "the handler has hand-written DTOs" (architecture/maintainability lens).

## Examples

- **V1.78 memory surface** (this doc's source): `handlers/memory.rs` had 14 hand-written structs (V1.33). V1.78 authored `schemas/local-api/memory/*.schema.json` (14 files), ran codegen (`@42ch/nexus-contracts` 0.12.0 → 0.13.0), normalized the handler to `pub use nexus_contracts::{...}`, hit Trap A (the `PendingReviewInfo` `query_as!`→`query!`+map bridge via `fetch_pending_reviews_by_creator` / `fetch_pending_reviews_page`) and Trap B (the generator `ambiguous_glob_reexports` allow). Round-trip test: `crates/nexus-daemon-runtime/tests/memory_dto_roundtrip.rs` (7 cases).
- **Contrast — the findings surface (V1.49/V1.77)** did it the other way: schemas were authored **with** the handler, so V1.77's findings-remediation UI consumed already-generated types (no gap, no normalization). The gap pattern is specifically about surfaces that shipped before their schemas.

## See Also

- [schemas-external-consumer-boundary.md](../schemas-external-consumer-boundary.md) — wire vs local-only contract types (external consumer side).
- [crate-selection-best-practices.md](../crate-selection-best-practices.md) — Rust workspace dependency conventions.
- `crates/nexus-daemon-runtime/AGENTS.md` — the no-hand-written-DTO invariant.

---
module: contracts-codegen
date: 2026-07-24
last_updated: 2026-07-24
problem_type: architecture-pattern
category: architecture-patterns
severity: high
tags: [contracts, codegen, json-schema, typify, json-schema-to-typescript, jstt, nexus-contracts, wire-drift, common-types]
applies_when: adopting or upgrading third-party schema-to-code generators; regenerating wire types after a codegen engine swap; diagnosing consumer breakage after `pnpm run codegen`
---

# Third-Party Codegen Adoption (jstt + typify)

Distilled from V1.138 P0 (TypeScript → `json-schema-to-typescript`) and P1 (Rust → `typify`). Replaces the retired bespoke `ts-generator.ts` / `rust-generator.ts` with library-driven emitters while preserving Nexus consumer shapes and the frozen `schemas/` wire contract.

## Context (V1.138)

Nexus wire types are generated from `schemas/**/*.schema.json` into:

- TypeScript — `packages/nexus-contracts/src/generated/`
- Rust — `crates/nexus-contracts/src/generated/`

Before V1.138, both languages used hand-rolled generators that encoded Nexus-specific quirks (flat re-exports, `SCHEMA_VERSIONS` stamps, skip lists, title overrides). V1.138 retired those generators in favor of industry-standard libraries:

| Language | Library | Orchestrator module |
| --- | --- | --- |
| TypeScript | `json-schema-to-typescript` (jstt) | `tooling/codegen/src/ts-gen.ts` |
| Rust | `typify` | `tooling/codegen/rust-gen/` (`nexus-rust-gen` binary) |

**What did not work:** `typify`'s `TypeSpaceSettings` cannot disable the shapes typify chooses to emit — newtyped string aliases, `chrono::DateTime` for `format: date-time`, `NonZeroU64` for `minimum: 1` integers, and prefixed string enums with derived `Display` / `FromStr`. The team attempted to tune settings to reproduce the bespoke generator's flatter output; typify does not expose knobs to turn those off. **Decision: accept typify's mapping and adapt consumers** rather than fork typify or bend schemas.

## Decision

1. **Libraries own type mapping.** jstt and typify decide how JSON Schema constructs become TS/Rust types. Nexus does not re-implement their logic in bespoke generators.
2. **Schemas stay frozen.** Wire contracts in `schemas/` are the SSOT. Do **not** edit schemas to preserve old generator quirks or to coerce typify into legacy shapes.
3. **Preserve Nexus public shapes where they matter.** `SCHEMA_VERSIONS`, `LATEST_SCHEMA_VERSION`, module tree layout, basename-derived PascalCase root type names, and skip-list behavior for definition-only schemas remain orchestrator-owned. Hand-maintained `common_types.rs` / `CommonTypes.ts` stay when un-skipping would fragment shared aliases across crates.
4. **Consumer adaptation is a first-class cost.** After drift tests pass, budget a cross-crate fix wave for `NonZeroU64`, `DateTime`, inlined struct copies per response, and enum trait changes.

## Pipeline

Orchestrator: `tooling/codegen/src/index.ts` (`pnpm run codegen`).

```
schemas/
    │
    ▼  stage 1 — schema-prep.ts
    ├─ localize  → tooling/codegen/.schemas-localized/   (POSIX-relative $id/$ref)
    └─ dereference → tooling/codegen/.schemas-dereferenced/  (self-contained trees)
    │
    ├─ stage 2 — ts-gen.ts (jstt)  → packages/nexus-contracts/src/generated/
    │
    └─ stage 3 — rust-gen/ (typify binary)  → crates/nexus-contracts/src/generated/
```

**Prep is shared.** Both emitters consume the dereferenced tree. Typify requires no cross-file `$ref`; jstt benefits from the same prep for consistent `$id` resolution.

**Rust isolation.** `rust-gen/` declares its own empty `[workspace]` so `typify` + `schemars` stay out of the root workspace graph. The orchestrator shells out via `cargo run --release` with env vars (`NEXUS_REPO_ROOT`, `NEXUS_DEREF_SCHEMAS_DIR`, `NEXUS_SRC_SCHEMAS_DIR`). See `tooling/codegen/rust-gen/AGENTS.md`.

**Title override.** Nexus schema `title` fields carry a `"Nexus …"` product prefix. Both emitters override the in-memory title to basename-derived PascalCase (`World`, `WorkSummary`, …) before generation so drift-test registrations and TS exports stay aligned. Source schema files are never mutated.

## Hard gates (success criteria)

Codegen adoption is **not** done when generated files are byte-identical to the old bespoke output. Done means:

| Gate | Command / artifact | What it proves |
| --- | --- | --- |
| Consumer compile | `cargo check --all`, `pnpm run typecheck` | Downstream crates and `@42ch/nexus-contracts` compile against new shapes |
| Lint | `cargo clippy --all -- -D warnings` | Typify clippy allows are scoped to `generated/` only; hand-written code stays clean |
| Wire drift | `./tooling/check-wire-drift.sh` | Every registered schema still round-trips through its generated Rust type (`schema_drift_detection`) |
| Schema validity | `pnpm run validate-schemas` | Input schemas are well-formed before codegen |

**Not a gate:** `git diff --exit-code` on generated sources matching pre-migration bytes. Library emitters will differ in derives, module layout, and helper impls. Semantic equivalence is proven by drift tests + consumer compile, not diff equality.

## Pitfalls

### 1. Keep hand-maintained `common_types.rs`

`common.schema.json` and `source-anchor.schema.json` remain on the skip list (`tooling/codegen/src/ts-gen.ts` → `SKIP_LIST`; mirrored in `rust-gen/src/main.rs` → `SKIP_SCHEMAS`). Typify would emit standalone structs/enums per definition if un-skipped, **fragmenting** types that dozens of crates import as `nexus_contracts::common_types::*` aliases.

**Rule:** keep the hand-maintained `common_types.rs` / `CommonTypes.ts` extraction for shared definitions. Only un-skip when the team is ready to migrate every `common_types::` import to typify's per-schema copies (V1.138 explicitly deferred this).

### 2. Typify emits `Display` / `FromStr` for string enums

Typify derives `Display` and `FromStr` on string enums. Hand-written duplicate impls in consumer crates become conflicting implementations.

**Fix:** remove hand-written `Display` / `FromStr` for wire enums; keep bespoke `as_str()` helpers only where call sites need a stable `&'static str` without allocating.

### 3. `DateTime` serialization changes canonical hashes

Typify maps `format: date-time` to `chrono::DateTime<…>`. Serde's default serialization for `DateTime` can differ from the bespoke generator's `String` RFC3339 fields — affecting canonical hash fixtures in cloud-sync specs.

**Residual (closed 2026-08-08):** `R-V1138P1-001` — spec golden hashes updated after the new serialization was verified wire-correct (no behavioral regression); see [`../../specs/canonical-hash.md`](../../specs/canonical-hash.md).

### 4. Consumer adaptation is mechanical but cross-crate

Typify inlines a distinct struct copy for every schema that references a shared definition (e.g. each daemon-api response gets its own `NexusSourceAnchor` copy, not `common_types::SourceAnchor`). Adapting consumers requires JSON round-trip bridges (`handlers/mod.rs` `wire_convert`), `NonZeroU64::new(…).unwrap()` at boundaries, and `DateTime::parse_from_rfc3339` in domain mappers.

**Rule:** land drift green first, then schedule a fix wave across `nexus-daemon-runtime`, `nexus-narrative`, `nexus-knowledge` (merged from `nexus-kb` V1.139), `nexus-orchestration`, and integration tests. Do not block codegen merge on every call-site polish if drift + clippy are green.

## Anti-patterns

| Anti-pattern | Why it fails |
| --- | --- |
| Editing `schemas/` to preserve old generator quirks | Breaks the frozen-wire invariant; external consumers and platform lock on schema truth |
| Copying spoke repo public shapes blindly | Spoke and Nexus diverged in skip lists, title overrides, and `daemon-api` tree layout — port the **pipeline pattern**, not output bytes |
| Suppressing clippy on hand-written code to "match green" | Typify allows belong **only** under `crates/nexus-contracts/src/generated/`; hand-written `#[allow(…)]` hides real consumer bugs |
| Expecting `TypeSpaceSettings` to disable newtypes / DateTime / NonZeroU64 | Confirmed non-viable in V1.138 — adapt consumers instead |
| Un-skipping `common.schema.json` without a migration plan | Fragments `common_types` aliases across the workspace |

## When to Apply

- Onboarding a new schema-to-code library or major version bump of jstt/typify.
- A `pnpm run codegen` diff changes derives, field types, or module layout without a schema edit.
- Consumer crates fail to compile after regeneration — check `NonZeroU64`, `DateTime`, enum traits, and inlined struct copies first.
- Adding a new schema: register it in `schema_drift_detection.rs` and run `./tooling/check-wire-drift.sh`.

## Related

| Resource | Role |
| --- | --- |
| [`tooling/codegen/`](../../../tooling/codegen/) | Pipeline orchestrator, prep, ts-gen, rust-gen binary |
| [`tooling/codegen/README.md`](../../../tooling/codegen/README.md) | Stage table, env contract, output layout |
| [`crates/nexus-contracts/AGENTS.md`](../../../crates/nexus-contracts/AGENTS.md) | Generated-crate rules, `enum_conversions.rs` |
| [`schemas/AGENTS.md`](../../../schemas/AGENTS.md) | Schema authoring + codegen flow |
| [`contracts-gap-on-shipped-backend.md`](contracts-gap-on-shipped-backend.md) | Closing schema gaps on shipped handlers (orthogonal but same contracts boundary) |
| Residuals `R-V1138P0-*` | Closed 2026-08-08 (V1.155 P2 residual sweep) |
| Residual `R-V1138P1-001` | Canonical-hash spec golden sync — closed 2026-08-08; spec at [`../../specs/canonical-hash.md`](../../specs/canonical-hash.md) |

## Evidence

- Shipped: V1.138 (per-version snapshot in the local roadmaps shipped tracker) — bespoke generators retired; library-driven `pnpm run codegen`; drift + workspace gates green.
- Plan: V1.138 P0 (jstt) + P1 (typify); integration branch `iteration/v1.138`.

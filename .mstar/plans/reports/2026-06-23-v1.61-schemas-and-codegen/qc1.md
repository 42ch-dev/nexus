---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-23-v1.61-schemas-and-codegen"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk (Seat 1)
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-23-v1.61-schemas-and-codegen
- Review range / Diff basis: iteration/v1.61..feature/v1.61-schemas-and-codegen
- Working branch (verified): feature/v1.61-schemas-and-codegen
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 22 (4 new `schemas/compute/*.schema.json`, extended `schemas/domain/key-block.schema.json` + `schemas/README.md`, 4 new Rust generated files, 4 new TypeScript generated files, extended `generated/mod.rs` + `KeyBlock.ts` + `index.ts`, extended `tests/schema_drift_detection.rs`, extended `.mstar/status.json`, extended plan markdown)
- Commit range: 4 commits since `iteration/v1.61` base (`3ac4ccb0`, `decb5e98`, `3244679c`, `c3c518ac`, `a7abbb22`, `9b5450fd`, `69df8a19`, `e64f567f`)
- Tools run:
  - `pnpm run validate-schemas` → 58 valid / 0 invalid (4 new compute schemas validated)
  - `tooling/check-wire-drift.sh` → 4 tests pass, 56 schemas / 55 structs in <500ms
  - `cargo check -p nexus-contracts` → clean
  - `cargo clippy -p nexus-contracts -- -D warnings` → 0 warnings on default targets
  - `cargo test -p nexus-contracts --test schema_drift_detection` → 4/4 tests pass
  - git diff stat + cross-module grep + manual schema review

## Findings

### 🔴 Critical

None.

### 🟡 Warning

None.

### 🟢 Suggestion

- **S-001: `state_delta.op` inline enum collapses to `String` in Rust** → Promote to a typed `enum StateDeltaOp { Add, Sub, Set }` in P3 alongside state delta merge logic. The schema correctly models the enum (`enum: ["add", "sub", "set"]`), and TypeScript correctly emits `ComputeOutputStateDeltaOp = 'add' | 'sub' | 'set'`, but Rust codegen's known inline-enum-to-`String` mapping (`crates/nexus-contracts/src/generated/compute_output.rs` line 17: `pub op: String`) means P3 consumers lose compile-time guarantees on the operation set. Properly flagged in Completion Report as a P3 risk; not blocking because the schema remains the SSOT and any malformed `op` value would still be caught by downstream `apply_state_delta()` validation. Fix scope: P3 (state delta merge).

- **S-002: `EntityAttributes.attributes` and `EntityState.state` typed as `serde_json::Value`** → The per-`block_type` definitions (`CharacterAttributes`, `ItemAttributes`, etc.) are documented in `definitions` blocks but **not enforced** at the wire level. Consumers must manually map `block_type` → matching definition. This is a deliberate V1 envelope trade-off matching compass Q8 ("Module-declared fields via manifest.json") and matches how `battle_report` is intentionally freeform. Acceptable for V1; consider promoting to `oneOf` over `block_type` in V2 if wire-level polymorphism is desired.

- **S-003: `world_ref` and `narrative_state` inner properties all optional** → `world_ref` is required as a top-level field on `ComputeInput`, but its inner properties (`world_id`, `branch_id`, `timeline_head_event_id`) have no `required` constraint. A host could legitimately pass `{"world_ref": {}}` and force compute modules to defensively handle empty locators. Same for `narrative_state` (all inner fields optional, which is fine since the field is module-interpreted). Consider adding `"required": ["world_id"]` to the `world_ref.properties` object in a follow-up — at least one locator should be guaranteed. Not blocking for V1 since compute modules are expected to be defensive anyway, but minor maintainability improvement.

- **S-004: BlockType coverage gap** → 5 of 18 valid `BlockType` enum values are mapped to placeholder or fully-specified attribute/state definitions (`character` fully specified; `item`, `faction`, `ability`, `species` permissive placeholders). 13 values remain uncovered (`scene`, `organization`, `conflict`, `info_point`, `event`, `magic_system`, `technology`, `deity`, `level`, `economy_tier`, `dialogue`, `beat`, `act`). Properly tracked as `R-V161P0-INFO-001` for P1 closure. This is intentional scope (P0 only handles combat-relevant types) and is not a defect — just a known gap for P1 to address.

## Source Trace

- Finding ID: S-001
  - Source Type: git-diff + manual-reasoning
  - Source Reference: `crates/nexus-contracts/src/generated/compute_output.rs` line 17 (`pub op: String`) vs `schemas/compute/compute-output.schema.json` line 22 (`enum: ["add", "sub", "set"]`) vs `packages/nexus-contracts/src/generated/ComputeOutput.ts` line 15 (`ComputeOutputStateDeltaOp = 'add' | 'sub' | 'set'`)
  - Confidence: High

- Finding ID: S-002
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus-contracts/src/generated/entity_attributes.rs` line 19 (`pub attributes: Option<serde_json::Value>`) vs `schemas/compute/entity-attributes.schema.json` `definitions` (5 per-type definitions present but not polymorphic at wire level)
  - Confidence: High

- Finding ID: S-003
  - Source Type: manual-reasoning
  - Source Reference: `schemas/compute/compute-input.schema.json` lines 11-31 (`world_ref` has no inner `required`)
  - Confidence: Medium (depends on V1 host-call assumptions)

- Finding ID: S-004
  - Source Type: doc-rule + manual-reasoning
  - Source Reference: `schemas/common/common.schema.json` line 101 (BlockType enum has 18 values); `schemas/compute/entity-attributes.schema.json` `definitions` only defines 5; tracked in `.mstar/status.json` `residual_findings[2026-06-23-v1.61-schemas-and-codegen][R-V161P0-INFO-001]`
  - Confidence: High

## Architecture & Maintainability Assessment

### Schema design coherence with locked compass §0 grill decisions

| Q# | Decision | Implementation | Verdict |
|----|----------|----------------|---------|
| Q3 | V1 envelope Schema; 4 JSON Schema files in `schemas/compute/` | 4 new files: `compute-input`, `compute-output`, `entity-attributes`, `entity-state` | ✅ Aligned |
| Q4 | attributes (static) + state (dynamic) + computable flag | All three present; `state` and `computable` added to `KeyBlockBody`; `EntityAttributes.attributes` is static params; `EntityState.state` is dynamic runtime | ✅ Aligned |
| Q5 | A: nested by block_type (`state.character.current_hp`) | `EntityState` `description` and `state` field structure (freeform object keyed by block_type) correctly models nesting; documented in field description | ✅ Aligned (note: not enforced at wire level — see S-002) |
| Q8 | 4-part output envelope (state_delta, timeline_events, new_key_blocks, battle_report) | All 4 present in `compute-output.schema.json` as required top-level fields with correct types | ✅ Aligned |
| Q11 | key-block extended (additive); 4 new compute schemas | `state` and `computable` added to `body.properties` (not in `required`); Rust `body: Option<serde_json::Value>` unchanged; TS `body?` unchanged; top-level `required` untouched | ✅ Truly additive — verified structurally |

### `$ref` structure and naming consistency

- All 4 new schemas use `https://nexus42.invalid/schemas/...` consistently with `schemas/AGENTS.md` §"Schema URI Placeholder"
- Cross-references use the `$ref` pattern: `common.schema.json#/definitions/SchemaVersion`, `common.schema.json#/definitions/WorldId`, `common.schema.json#/definitions/TimelineEventId`, `common.schema.json#/definitions/KeyBlockId`, `common.schema.json#/definitions/BlockType`, `domain/key-block.schema.json`, `domain/timeline-event.schema.json`
- No `$ref` to local relative paths (all use the absolute `nexus42.invalid` form) — consistent with existing schemas
- File naming: kebab-case (`compute-input.schema.json`) → matches existing `entity-state`, `key-block`, `timeline-event` patterns
- Rust module naming: snake_case from filename (`compute_input.rs`) → matches existing pattern
- Type naming: PascalCase (`ComputeInput`, `ComputeOutput`, `EntityAttributes`, `EntityState`, `CharacterAttributes`, `CharacterState`, `ComputeOutputStateDelta`) → matches existing pattern

### Additive-only invariant (structural verification)

Verified at three levels:

1. **Schema level** (`schemas/domain/key-block.schema.json`):
   - Top-level `required` array (line 8): `["schema_version", "key_block_id", "world_id", "block_type", "canonical_name", "status", "created_at"]` — **unchanged** from prior version
   - `body.properties` extended with `state` and `computable` — **neither** added to `body.properties.required`
   - `body` itself is **not** in top-level `required`
   - Top-level `additionalProperties: false` preserved

2. **Rust type level** (`crates/nexus-contracts/src/generated/key_block.rs`):
   - `pub body: Option<serde_json::Value>` — **unchanged**
   - All other fields unchanged
   - Confirmed by `git diff` showing only `KeyBlock.ts` (TS) has the body property shape extended; the Rust struct itself had no diff lines

3. **TypeScript type level** (`packages/nexus-contracts/src/generated/KeyBlock.ts`):
   - `body?: { summary?: string; attributes?: Record<string, unknown>; tags?: string[]; state?: Record<string, unknown>; computable?: boolean }` — extension is purely additive (new optional keys)
   - Top-level required fields unchanged

Conclusion: a pre-V1.61 KeyBlock instance (no `state`, no `computable`) remains valid under the new schema; legacy deserialization paths continue to work without code changes.

### Generated code quality

- **Rust** (`crates/nexus-contracts/src/generated/`): All 4 new modules follow the existing struct generation pattern: `#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]`, `#[serde(rename_all = "snake_case")]`, module-level doc comment with `@schema_version` and `@source`, inline type alias for `ComputeOutputStateDelta`. Cross-module imports follow the same `use crate::generated::...` style. No hand-edits visible.
- **TypeScript** (`packages/nexus-contracts/src/generated/`): All 4 new interfaces follow the existing pattern with JSDoc-style comments, `import type` for cross-module references, and the dedicated `ComputeOutputStateDeltaOp` union type correctly preserving the schema's enum constraint.
- **Module exports**: `crates/nexus-contracts/src/generated/mod.rs` adds the 4 new modules to both `pub mod` and `pub use *` blocks, and registers them in `SCHEMA_VERSIONS` with version 1. `packages/nexus-contracts/src/generated/index.ts` adds the 4 new exports and `SCHEMA_VERSIONS` entries.
- **No TODOs/FIXMEs**: `grep -c "TODO\|FIXME\|XXX"` returns 0 across all 4 new generated files.

### Schema drift detection registration

All 4 new schemas registered in `build_schema_map()` at `crates/nexus-contracts/tests/schema_drift_detection.rs` lines 139-163 with `Strict` mode and informative comment:

```rust
// ── compute/ ────────────────────────────────────────────────────
// V1.61 WASM compute ABI envelopes (compass Q3/Q8). Only the top-level
// struct of each schema is registered; inline/definition structs
// (ComputeOutputStateDelta, CharacterAttributes, CharacterState) are
// emitted by codegen but validated indirectly via their parent schema.
entry!("schemas/compute/compute-input.schema.json", Strict, ComputeInput),
entry!("schemas/compute/compute-output.schema.json", Strict, ComputeOutput),
entry!("schemas/compute/entity-attributes.schema.json", Strict, EntityAttributes),
entry!("schemas/compute/entity-state.schema.json", Strict, EntityState),
```

This matches the existing pattern (e.g., `Bundle` entry with the same explanatory comment about `allOf` skipping) and is correctly placed in the alphabetical-by-folder ordering between `domain/` and `common/`.

**Drift detection result**: PASS — 56 schemas, 55 structs, <500ms threshold (per test stdout).

### Maintainability for downstream P1/P2/P3 consumers

| Plan | Downstream usage of P0 artifacts | Clarity assessment |
|------|----------------------------------|---------------------|
| P1 (KB structured layer) | Extends `crates/nexus-kb/src/key_block.rs` local `KeyBlockBody` with `state` + `computable` (mirroring the wire schema) | ✅ Clear — schema provides SSOT shape |
| P2 (wasm-host + basic-combat) | Implements `compute(module_bytes, input: ComputeInput) → ComputeOutput` against the generated wire types | ✅ Clear — Rust types are strongly-typed except for the S-001 caveat |
| P3 (narrative.compute capability) | Implements `apply_state_delta()` against `state_delta` items, surfaces `battle_report` to consumers | ✅ Clear — schema documents the merge algorithm as P3-scope and the freeform battle_report intent |

The plan correctly stops at the schema foundation; no premature implementation leaks into P2/P3 territory.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

The P0 plan delivers exactly what the locked compass §0 decisions specified, with all schemas well-formed, codegen output coherent and consistent with existing patterns, and the additive-only invariant preserved at the schema, Rust type, and TypeScript type levels. All four CI gates pass cleanly (`validate-schemas` 58/58, `check-wire-drift.sh` 4/4 tests, `cargo check -p nexus-contracts`, `cargo clippy -p nexus-contracts -- -D warnings`). The 4 suggestions are non-blocking improvements for downstream plans (P1/P2/P3) and the implementation correctly stops at the foundation boundary.
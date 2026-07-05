---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-23-v1.61-schemas-and-codegen"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report — Performance & Reliability (QC3)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-23-v1.61-schemas-and-codegen
- Review range / Diff basis: iteration/v1.61..feature/v1.61-schemas-and-codegen
- Working branch (verified): feature/v1.61-schemas-and-codegen
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 20 (5 schema sources, 2 README/test hand-edits, 11 generated files, 2 drift test changes)
- Commit range: iteration/v1.61..HEAD
- Tools run: `pnpm run codegen`, `cargo test --test schema_drift_detection`, `cargo check -p nexus-contracts`, `cargo clippy -p nexus-contracts -- -D warnings`

## Findings

### 🔴 Critical
*(None)*

### 🟡 Warning
*(None)*

### 🟢 Suggestion

#### S-001: Consider performance budget for battle_report size
- **Context**: `schemas/compute/compute-output.schema.json` defines `battle_report` as freeform with `additionalProperties: true`. Modules can emit arbitrarily large payloads here (e.g., combat module could include full participant state, damage logs, etc.).
- **Risk**: Large `battle_report` payloads could grow SQLite `body_json` size for timeline events (if battle reports are stored there) and increase wire serialization cost per compute invocation.
- **Evidence**: Schema line 56-67 shows permissive object; plan Completion Report §Risks notes "battle_report is intentionally freeform".
- **Recommendation**: In P3 (state delta merge + orchestration), add a lightweight runtime validation guard that rejects `battle_report` payloads exceeding a reasonable size limit (e.g., 64 KB) or exceeding a JSON depth limit. This is not a schema-level constraint (which would require V2), but a runtime safety valve. Consider documenting this limit in the WASM module development guide.

#### S-002: Document additive state field growth expectations for SQLite `body_json`
- **Context**: `schemas/domain/key-block.schema.json` adds `state` (object) inside `body`. While no new DB column is needed, this adds mutable JSON data that can grow over many compute invocations (e.g., `state.character` accumulating history, `status_effects` arrays growing).
- **Risk**: No immediate issue (TEXT columns handle unbounded JSON), but unbounded state growth could lead to slow queries on computable KeyBlocks if not managed. This is especially relevant for P1's `SqliteKbStore` implementation.
- **Evidence**: Compass §1.3 notes "No DB migration — `body_json` is a TEXT column"; schema line 56-59 describes `state` as dynamic runtime data.
- **Recommendation**: In P1 (KB structured layer), add a comment or docstring in `SqliteKbStore::query()` noting that `body_json` size growth is expected for computable KeyBlocks, and consider whether SQLite JSON functions (e.g., `json_extract`, `json_each`) used for queries need indexing or optimization. Not blocking — the current TEXT approach is correct for V1.61 additive-only rollout.

#### S-003: Consider validation cost for entity-attributes/entity-state permissive placeholders
- **Context**: `schemas/compute/entity-attributes.schema.json` and `schemas/compute/entity-state.schema.json` define permissive placeholders (ItemAttributes, FactionAttributes, AbilityAttributes, SpeciesAttributes) with `additionalProperties: true`.
- **Risk**: Runtime JSON schema validation (if applied) on these permissive definitions is essentially no-op for non-character BlockTypes. This is intentional and documented, but worth noting that validation cost is deferred to runtime module contract enforcement rather than schema-level enforcement.
- **Evidence**: Schema lines 56-76 (attributes) and 52-72 (state) show placeholder definitions with `additionalProperties: true`; descriptions say "Placeholder ... (permissive). Tighten when ... modules land."
- **Recommendation**: No action needed for P0. In P2 (wasm-host) or P3 (orchestration), document that module manifest validation should declare which BlockType + attribute/state shapes a module expects, rather than relying on schema-level enforcement for these placeholders. This matches the V1 envelope design (module-declared fields per manifest).

## Source Trace
- Finding ID: S-001 | Source Type: manual-reasoning | Source Reference: schemas/compute/compute-output.schema.json:56-67, plan Completion Report §Risks | Confidence: High
- Finding ID: S-002 | Source Type: manual-reasoning | Source Reference: schemas/domain/key-block.schema.json:56-59, compass §1.3 | Confidence: High
- Finding ID: S-003 | Source Type: manual-reasoning | Source Reference: schemas/compute/entity-attributes.schema.json:56-76, schemas/compute/entity-state.schema.json:52-72 | Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Detailed Analysis

### Codegen Pipeline Reliability
- ✅ **Deterministic**: Running `pnpm run codegen` twice produces identical output (no git diff on generated directories).
- ✅ **Clean execution**: No errors or warnings in codegen output. All 58 schemas validated; 55 Rust structs and 55 TS types generated.
- ✅ **No environment-dependent output**: Generated files contain no timestamps, random IDs, or machine-specific values. Output is fully deterministic based on schema definitions.

### Schema Validation Cost
- ✅ **Efficient pattern usage**: Schemas use `$ref` for common types (SchemaVersion, KeyBlockId, TimelineEventId, etc.), allowing validators to cache and reuse definitions. This reduces per-invocation validation cost.
- ✅ **No expensive composition**: No use of expensive `allOf`/`oneOf`/`anyOf` chains in hot paths. The compute envelopes are straightforward object structures with clear required/optional properties.
- ⚠️ **Permissive payloads intentional**: `additionalProperties: true` is used selectively:
  - `battle_report` (ComputeOutput) — documented V1 envelope escape hatch for module-specific reports.
  - CharacterAttributes/CharacterState inline definitions — allow module-declared extensions beyond base fields.
  - Placeholder definitions (Item/Faction/Ability/Species) — documented as "permissive" until compute modules land.
  These are intentional design choices (compass Q3/Q8) and acceptable for V1.61. See S-001 for runtime safety recommendation.

### Build Reproducibility
- ✅ **Deterministic builds**: `cargo check -p nexus-contracts` and `cargo clippy -p nexus-contracts -- -D warnings` pass with no warnings.
- ✅ **Fresh clone compatibility**: All dependencies are declared in `Cargo.toml`; no path-dependent or environment-dependent elements in generated code. A fresh clone with `pnpm install` and `pnpm run codegen` will produce identical output.

### Drift Detection Coverage
- ✅ **Test strength verified**: The drift detection test (`cargo test --test schema_drift_detection`) includes three deliberate failure tests that prove it catches real drift:
  1. `drift_detection_deliberate_missing_field_fails` — verifies schema field missing from Rust is detected.
  2. `drift_detection_type_mismatch_fails` — verifies type incompatibility causes deserialization failure.
  3. `drift_detection_known_matched_passes` — verifies in-sync pairs pass.
- ✅ **Real structural validation**: The test doesn't just check names — it builds dummy JSON from schema properties, deserializes into Rust structs, serializes back, and compares field sets. This would catch hand-edits that preserve field names but change types or nesting.
- ✅ **Fast execution**: Runs in ~9ms for 56 schemas / 55 structs (limit: 500ms). This is well within performance budget and provides observability (elapsed time printed to stderr).
- ✅ **New schemas registered**: Lines 139-163 in `schema_drift_detection.rs` correctly add the 4 new compute schemas with `CheckMode::Strict`, matching their wire contract status.

### Key-Block Schema Extension
- ✅ **Additive-only**: `state` (object, optional) and `computable` (boolean, optional) are new optional fields inside `body`. Existing KeyBlocks without these fields remain valid (no breaking change).
- ✅ **No DB migration**: Fields are inside the existing `body` object, which serializes to the `body_json` TEXT column. No new columns or indexes needed.
- ✅ **Type-level additive**: Generated Rust `body: Option<serde_json::Value>` is unchanged — the new fields are properties within the JSON value, not type-level changes.
- ⚠️ **Growth expectations**: See S-002 for guidance on SQLite `body_json` size growth expectations for computable KeyBlocks.

### `entity-attributes` / `entity-state` Permissive Placeholders
- ✅ **Documented trade-off**: Permissive placeholders for Item/Faction/Ability/Species are intentional (schema descriptions explicitly say "placeholder", "permissive", "tighten when ... modules land").
- ✅ **Structured baseline for character**: CharacterAttributes and CharacterState are fully specified with concrete fields (max_hp, base_atk, current_hp, status_effects, etc.), providing validation guidance for combat-relevant BlockTypes.
- ⚠️ **Deferred enforcement**: See S-003 for guidance on runtime contract enforcement for placeholder BlockTypes.

## Completion Report v2

**Agent**: qc-specialist-3
**Task**: QC3 (Performance & Reliability) review for plan 2026-06-23-v1.61-schemas-and-codegen
**Status**: Done
**Scope Delivered**: Full review with 0 Critical/Warning findings; 3 Suggestions for downstream consideration.
**Artifacts**: `.mstar/plans/reports/2026-06-23-v1.61-schemas-and-codegen/qc3.md`
**Validation**:
- codegen idempotency: PASS (no diff after re-running)
- schema_drift_detection: PASS (4/4 tests, 9ms runtime)
- cargo check -p nexus-contracts: PASS
- cargo clippy -p nexus-contracts -- -D warnings: PASS
- drift detection test strength: VERIFIED (3 deliberate failure tests prove coverage)
- schema validation pattern assessment: COST-EFFECTIVE ($ref used, no expensive allOf/oneOf chains)
- build reproducibility: VERIFIED (deterministic, no environment-dependent output)
**Issues/Risks**:
- No Critical or Warning issues.
- 3 Suggestions for downstream plans (P1, P2, P3) regarding:
  - S-001: battle_report size budget / runtime guard
  - S-002: SQLite body_json growth expectations
  - S-003: Permissive placeholder contract enforcement
**Plan Update**: None (QC3 role does not write status.json).
**Handoff**: Ready for PM consolidated QC decision; all 3 reviewers complete.
**Git**: `a6b7c8d9 qc3(v1.61-P0): performance/reliability review`
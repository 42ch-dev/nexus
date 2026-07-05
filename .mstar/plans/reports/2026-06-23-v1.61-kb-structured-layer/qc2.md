---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-23-v1.61-kb-structured-layer"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness (focus per role parameters)
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-23-v1.61-kb-structured-layer
- Review range / Diff basis: d268f8e6..feature/v1.61-kb-structured-layer
- Working branch (verified): iteration/v1.61
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 20 (core: key_block.rs, validation.rs, query.rs, kb_store.rs + supporting)
- Commit range: 0da0fb3a (feat(kb): add state/computable fields, Structured validation, and computable query filter)
- Tools run: git diff, git branch/rev-parse, cargo test -p nexus-kb, cargo test -p nexus-local-db, source inspection of json_extract usage, manual construction of legacy + computable KeyBlockBody instances

## Compass Alignment (§0 Q4/Q5)
- Q4: KB structured layer = **attributes (static) + state (dynamic) + computable flag**. Per-BlockType JSON Schema. (Verified in doc comments, KeyBlockBody fields, and validation logic.)
- Q5: KB state granularity = **nested by block_type** (`state.character.current_hp`). (Verified: `block_type_state_key()` + `validate_structured_body` gate 3 + test data.)

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None.

### 🟢 Suggestion
- (Low) The `json_extract(body_json, '$.computable')` expression exists **only inside a documentation comment** in `SqliteKbStore::query()` recommending a future deferred index. No executed SQL path uses dynamic `json_extract` or string interpolation of the filter. Current implementation correctly deserializes to typed `KeyBlockBody` then applies an in-memory `computable` filter (consistent with all other KbQuery filters). This is injection-safe by construction. When the index is later added, ensure it is created via a migration (static DDL) and not via runtime string building.

## Specific Security & Correctness Gates (per assignment)

1. **Additive invariant proof (legacy KeyBlockBody without state/computable)**:
   - `KeyBlockBody` derives `Default`.
   - `From<nexus_contracts::KeyBlock>` uses `unwrap_or_else(|_| KeyBlockBody::default())`.
   - Explicit test `test_state_roundtrip_without_state_and_computable` constructs a legacy body (only `summary`), serializes, deserializes, and asserts `state == None`, `computable == None`, and original `summary` preserved.
   - All new fields are `Option<T>` with `#[serde(skip_serializing_if = "Option::is_none")]`.
   - **PASS** — legacy blocks validate and roundtrip identically.

2. **`ValidationMode::Structured` correctness**:
   - `validate_structured_body` only enters validation when `b.computable == Some(true)`; otherwise returns `Ok(())` immediately.
   - Non-computable blocks (`None` or `Some(false)`) and `None` body are explicitly skipped (backward compatible).
   - For `Some(true)`: requires `attributes` (object), `state` (object), and (for canonical computable BlockTypes) the per-block_type nested key (`character`/`item`/etc.).
   - Tests cover: skips non-computable, skips `computable: false`, accepts `None` body, accepts valid computable for Character/Item/Faction/Ability/Species, rejects missing state/attrs/bad state key.
   - **PASS** — correctly validates computable blocks **and** skips non-computable.

3. **`json_extract(body_json, '$.computable')` SQL correctness + injection safety**:
   - No runtime `json_extract` appears in executed queries for this feature.
   - Base fetch uses `list_by_world` (static `sqlx::query_as!` compile-time checked).
   - `computable` filter is pure Rust post-deserialization: `kb.body.as_ref().and_then(|b| b.computable).unwrap_or(false)`.
   - The string literal exists **only** in a `//` doc comment as a recommended future index.
   - No user input is ever interpolated into SQL for this filter.
   - **PASS** — no injection surface introduced; current path is type-safe after deserialization.

4. **State roundtrip integrity**:
   - `test_state_roundtrip_serialize_deserialize_preserves_state` uses deeply nested state (`state.character.current_hp`, `status_effects` array, `position`, `is_alive`).
   - Additional tests cover empty `state.character {}` object and legacy absence.
   - Serde roundtrip + `PartialEq` + explicit field assertions.
   - **PASS** — nested state preserved; no silent data loss.

5. **No silent data loss on new fields**:
   - All construction sites in tests use `..Default::default()`.
   - Contract conversion falls back to default (preserves what it can).
   - Optional fields + skip_serializing_if ensure absent fields do not appear in JSON.
   - **PASS**.

## Source Trace
- Finding ID: N/A (no blocking findings)
- Source Type: manual code review + executed tests + git diff
- Source Reference: `git diff d268f8e6..feature/v1.61-kb-structured-layer`, `crates/nexus-kb/src/{key_block.rs,validation.rs,query.rs}`, `crates/nexus-local-db/src/kb_store.rs`, compass §0 Q4/Q5
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 (non-blocking documentation note) |

**Verdict**: Approve

## Artifacts Inspected
- Compass: `.mstar/iterations/v1.61-programmable-narrative-progression-delivery-compass-v1.md` §0 (Q4/Q5)
- Plan: `.mstar/plans/2026-06-23-v1.61-kb-structured-layer.md`
- Diff range verified on `iteration/v1.61` at review time.
- Test runs: `cargo test -p nexus-kb` (139 passed), `cargo test -p nexus-local-db` (KB-related tests passed).

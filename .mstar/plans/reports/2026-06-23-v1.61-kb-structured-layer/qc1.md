---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-23-v1.61-kb-structured-layer"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist (Seat 1)
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk (KeyBlockBody state/computable field design; ValidationMode::Structured; KbQuery::with_computable; block_type_state_key helper; construction-site completeness)
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-23-v1.61-kb-structured-layer
- Review range / Diff basis: d268f8e6..feature/v1.61-kb-structured-layer
- Working branch (verified): iteration/v1.61
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 20 (KeyBlockBody struct + 6 impl modules + 13 construction/test sites)
- Commit range: d268f8e6..0da0fb3a (single commit `feat(kb): add state/computable fields, Structured validation, and computable query filter`)
- Tools run: git diff d268f8e6..feature/v1.61-kb-structured-layer; cargo check -p nexus-kb -p nexus-local-db; cargo clippy -p nexus-kb -p nexus-local-db -- -D warnings; cargo test -p nexus-kb --lib (139 passed, +21 new); cargo test -p nexus-local-db --lib (271 passed); cargo test -p nexus-contracts --test schema_drift_detection (4 passed)

## Findings

### 🔴 Critical
*(none)*

### 🟡 Warning
*(none)*

### 🟢 Suggestion
- S-001 — Spec amendment for `entity-scope-model.md §5.5` is not yet committed: Compass V1.61 §2 marks the `entity-scope-model.md §5.5` row "Amend — computable flag semantics + structured validation mode" as P1 normative work, but the spec doc (last updated V1.55 P3, status line at line 7) does not yet contain a §5.5.9 / equivalent subsection documenting the computable flag semantics, the `ValidationMode::Structured` rules, or the per-`block_type` state-key nesting. This is a process/doc-tracking item rather than a code defect — P1 implementation is correct and complete — but the spec amendment should land before P-last to keep the normative doc in lockstep with the implementation. -> Suggested fix (PM/process): add a §5.5.x subsection to `entity-scope-model.md` documenting (a) the canonical computable BlockType set (Character/Item/Faction/Ability/Species — note that `environment` is NOT a wire enum variant, see R-V161P0-INFO-001), (b) the `state` nested-by-block_type grammar (Q5), (c) the structured-validation rules (attributes/state present + correct nested key), (d) the additive/backward-compatibility invariant for legacy KeyBlocks. Owner: PM or @product-manager. Cross-reference: `block_type_state_key()` doc-comment at `crates/nexus-kb/src/validation.rs:597-614` and `KeyBlockBody` doc-comment at `crates/nexus-kb/src/key_block.rs:14-33` already document the same intent at the code-doc level — the spec amendment is the normative-doc mirror.

## Source Trace
- Finding ID: S-001
- Source Type: doc-rule + manual-reasoning
- Source Reference: `.mstar/iterations/v1.61-programmable-narrative-progression-delivery-compass-v1.md` §2 normative specs row "entity-scope-model.md §5.5 — Amend — computable flag semantics + structured validation mode — P1" vs current `.mstar/knowledge/specs/entity-scope-model.md` (no V1.61 §5.5.x amendment; last updated V1.55 P3)
- Confidence: High (compass line 142 names P1 as the owner of this amendment; doc last-updated header at line 7 confirms no V1.61 entry)

## Architecture / Maintainability Assessment

### KeyBlockBody state/computable field design
**Aligned with compass Q4/Q5 (✓ Approve).** The struct adds `state: Option<serde_json::Value>` and `computable: Option<bool>` as additive optional fields with `#[serde(skip_serializing_if = "Option::is_none")]` on all body fields. This is the textbook additive-extension pattern: legacy `body_json` lacking the new keys deserializes cleanly because `serde_json::from_value(v).unwrap_or_else(|_| KeyBlockBody::default())` (line 314) handles both schema-compliant input and degraded legacy rows. The module-level doc-comment at `key_block.rs:14-33` correctly traces back to compass Q4 (attributes vs state vs computable) and Q5 (nested-by-block_type), and explicitly documents the canonical computable BlockType set.

**Wire/local separation is clean (✓ Approve).** Wire `nexus_contracts::KeyBlock.body` remains `Option<serde_json::Value>` (untouched); only the local `nexus_kb::KeyBlockBody` is typed. The `From<nexus_contracts::KeyBlock> for KeyBlock` conversion (line 302-325) correctly routes through serde for the body. The schema drift detection suite (4/4 passing) confirms no wire/local drift introduced by this change.

**P3 consumption surface is complete (✓ Approve).** All three new fields are `pub` on `KeyBlockBody`. `block_type_state_key()` is `pub const fn` (compile-time evaluable, good for hot-path use) and re-exported via `lib.rs:35-37`. `ValidationMode::Structured` is part of the `ValidationMode` enum and follows the same Display/serde conventions as `Novel`/`GameBible`/`Script`. `KbQuery::with_computable(Option<bool>)` is `pub const fn` and consistent with the other `with_*` builders.

### ValidationMode::Structured enum design
**Consistent with sibling variants (✓ Approve).** The new variant follows the established shape: skips when not applicable (non-computable, computable=false, no body), enforces structured-shape invariants (attributes/state must be present and be JSON objects), and rejects with typed `ValidationKind` variants (`MissingStructuredAttributes`, `MissingStructuredState`, `NonObjectStructuredState`, `InvalidStructuredStateKey`). Error display strings follow the same `snake_case` convention. The `validate_structured_body` function (validation.rs:530-597) correctly orders its three gates: attributes → state-object → per-block_type key. Non-computable BlockTypes (Scene/Event/etc.) for which `block_type_state_key()` returns `None` are allowed any state shape — this matches the wire schema's `additionalProperties: true` semantics for non-character block types.

**Completeness of `block_type_state_key()` match arms (✓ Approve).** The match is exhaustive over all 18 wire `BlockType` variants (verified against `crates/nexus-contracts/src/generated/common_types.rs:120-140`): 5 computable → `Some("character"/"item"/"faction"/"ability"/"species")`; 13 non-computable → `None`. No `#[non_exhaustive]` escape hatch needed because the enum is closed in the generated wire types. The computable set matches the P0 schemas `entity-attributes.schema.json` / `entity-state.schema.json` definitions (Character/Item/Faction/Ability/Species all have `*State` definitions; the four non-character types are permissive placeholders with `additionalProperties: true`).

**Production wiring is intentionally deferred (✓ Note for PM).** `ValidationMode::Structured` is not yet routed into `kb_extract_work.rs` (lines 340-348 only branch on `Novel` vs `Generic`) or any other production path. This matches the plan topology — P3 (`compute-capability-and-preset`) will wire it into the `combat-engine` preset's adoption path. The new variant is exercised by 12 unit tests in `validation.rs` and 4 InMemory + 4 SQLite store tests, so its behavior is locked-in for P3 to consume without ambiguity.

### KbQuery::with_computable API coherence
**Consistent with existing builder methods (✓ Approve).** `with_computable(Option<bool>)` follows the exact pattern of `with_block_type(BlockType)`, `with_text_search(text)`, `with_limit(usize)`, `with_offset(usize)`: `#[must_use] pub const fn` returning `Self`, consuming `mut self`, mutating the matching field. The field default is `None` (no filter) so existing callers are unaffected. The doc comment correctly explains the three semantic states (`Some(true)` / `Some(false)` / `None`).

**Filter implementation is symmetric across stores (✓ Approve).** `InMemoryKbStore::query()` (store.rs:316-322) and `SqliteKbStore::query()` (kb_store.rs:527-534) both use identical `kb.body.as_ref().and_then(|b| b.computable).unwrap_or(false)` semantics — legacy KeyBlocks lacking the field are treated as non-computable (consistent with the validator's "skip non-computable" rule). The SQLite path is consistent with the existing strategy of fetching-all-and-filtering-in-memory; the in-memory filter is documented as correct for current V1.61 dataset sizes.

### block_type_state_key() helper design for P3 consumption
**Correct shape and visibility (✓ Approve).** The helper is `pub const fn` (compile-time evaluable → P3 can use it in `match` arms and static arrays), returns `Option<&'static str>` (zero-allocation → safe for hot paths), and is exhaustive over the BlockType enum (verified above). The doc-comment at validation.rs:597-614 documents the P1-T2 design decision, the non-computable fallback semantics, and traces back to compass open design item #5.

**R-V161P0-LOW-004 follow-through (✓ Approve).** The V1.61 P0 residual `R-V161P0-LOW-004` (open) requires P1 to document body_json growth expectation and assess json_extract indexing. The implementation handles this through a 21-line doc-comment at `kb_store.rs:473-493` explaining: (1) the body_json growth expectation, (2) the deferred SQLite expression index recommendation with the exact `CREATE INDEX … ON kb_key_blocks(json_extract(body_json, '$.computable'))` SQL, (3) the no-migration-required invariant, and (4) the in-memory filter consistency with sibling filters. This is exemplary cross-plan residual follow-through — the comment is canonical, copy-pastable, and includes the index SQL verbatim.

### Construction-site update completeness (~15 files)
**Plan said ~15 files; actual is 20 files; all consistent (✓ Approve).** Construction-site audit: total `KeyBlockBody {` occurrences across the monorepo: 76 (including the struct definition itself, production sites, and test fixtures). Production sites in the diff: `kb_extract_work.rs:355` (added `..Default::default()`). Test sites in the diff: 18 (all consistently use `..Default::default()`). Verified across the entire codebase via grep that **all** `KeyBlockBody { … }` literals either (a) use `..Default::default()` for the trailing fields, or (b) explicitly set the new `state:` and `computable:` fields (only the tests that intentionally exercise the new fields). No construction site was missed; the build (`cargo check -p nexus-kb -p nexus-local-db` and `cargo clippy -p nexus-kb -p nexus-local-db -- -D warnings`) passing confirms uniformity.

**`Default` derive on `KeyBlockBody` (✓ Approve).** Adding `#[derive(Default)]` (line 34) is the right choice — every existing fixture and production site gets the additive fields for free via `..Default::default()`. This is preferable to hand-rolling a `::default()` impl with explicit `None` for each field (which would be a maintenance burden for future additive extensions).

### Test coverage
**Comprehensive (✓ Approve).** 21 new tests:
- **3 roundtrip tests** in `key_block.rs` (state+computable present, both absent, empty state object).
- **4 InMemory filter tests** in `store.rs` (computable=true, computable=false, computable=None, legacy block with absent field).
- **4 SQLite filter tests** in `kb_store.rs` (mirrors of the InMemory tests).
- **13 validation tests** in `validation.rs` (12 covering accept/reject paths × 5 computable BlockTypes + 4 rejection scenarios + 1 Display, plus 1 helper mapping test).
- The `block_type_state_key` helper has 1 mapping test covering all 5 computable types and 2 non-computable types (Scene, Event).

**Test placement is consistent with prior validation modes (✓ Approve).** Structured-mode tests follow the same `make_computable_body`/`make_non_computable_body` fixture pattern as the existing Novel/GameBible/Script test fixtures in the same file, which makes the test surface self-documenting and easy to extend.

### Doc comment quality
**High quality, traces back to compass decisions (✓ Approve).** The module-level doc on `KeyBlockBody` (key_block.rs:14-33) names compass Q4/Q5 explicitly, lists the canonical computable BlockType set with rationale, and explains the nested-key convention. The `state` field doc-comment (key_block.rs:42-47) explains "nested by block_type to avoid field-name collisions across module types" and references the wire schema. The `computable` field doc-comment (key_block.rs:48-53) explains the design choice to store inside body_json (additive, migration-free). The `block_type_state_key` helper doc (validation.rs:597-614) is a 17-line normative spec for P3.

### Risk for P3 downstream consumption
**Low risk, well-prepared (✓ Approve).** The architecture gives P3:
1. A typed `state: Option<serde_json::Value>` field — P3 can apply `+/-/set` semantics via `serde_json::Value` mutators without hand-writing JSON path accessors.
2. A typed `computable: Option<bool>` flag — P3 can filter via `KbQuery::with_computable(Some(true))` to enumerate all computable KeyBlocks.
3. A `pub const fn block_type_state_key(BlockType) -> Option<&'static str>` helper — P3 can compute the expected state-key at compile time and validate state shape.
4. A `ValidationMode::Structured` variant — P3 can call `validate_body(bt, body, ValidationMode::Structured)` to enforce the structured invariants.
5. A typed `attributes: Option<serde_json::Value>` field — unchanged, but its presence+object requirement is now enforced under Structured mode.

The only ergonomic gap is the absence of a helper like `block_type_state_key()` for the `attributes` shape, but attributes doesn't have a per-block_type nesting requirement (it's flat per the schema), so a helper would be redundant.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

The implementation is architecturally sound, well-aligned with compass Q4/Q5, provides a complete public surface for P3, follows the established sibling-variant patterns (Novel/GameBible/Script), and demonstrates exemplary cross-plan residual follow-through (R-V161P0-LOW-004). The single Suggestion is a doc-tracking item (spec amendment to `entity-scope-model.md §5.5`) that does not affect the P1 implementation and is appropriately owned by PM/process rather than the implementer.
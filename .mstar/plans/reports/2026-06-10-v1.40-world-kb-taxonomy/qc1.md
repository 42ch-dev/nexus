---
report_kind: qc_review
reviewer: "@qc-specialist"
reviewer_index: 1
plan_id: "2026-06-10-v1.40-world-kb-taxonomy"
verdict: "Approve"
generated_at: "2026-06-10T12:00:00Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: architecture coherence and maintainability risk
- Report Timestamp: 2026-06-09T12:00:00Z

## Scope
- plan_id: 2026-06-10-v1.40-world-kb-taxonomy
- Review range / Diff basis: iteration/v1.40..feature/v1.40-world-kb-taxonomy (df7f256b..8f9a5efc)
- Working branch (verified): feature/v1.40-world-kb-taxonomy
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 6
- Commit range: df7f256b..8f9a5efc
- Tools run: cargo test -p nexus-kb (68 passed), cargo test -p nexus-local-db (3 passed), cargo clippy -p nexus-kb (clean), cargo clippy -p nexus-local-db (clean)

## Findings
### 🔴 Critical
- **C-001: SqliteKbStore has no body validation — production path unprotected** (`crates/nexus-local-db/src/kb_store.rs:176-231`, `crates/nexus-local-db/src/kb_store.rs:408-476`)
  Plan T3 explicitly requires "Wire validation into KbStore insert/update paths (SqliteKbStore)". The implementation only wires `validate_body()` into `InMemoryKbStore` (the test-only store). The production path — `SqliteKbStore::insert_key_block()` (line 177) and `SqliteKbStore::update_key_block()` (line 408) — has zero body validation. This means:
  - CLI/daemon users creating novel-profile KeyBlocks get no `novel_category` enforcement
  - The `kb-extract` orchestration workflow (`kb_extract_work.rs:316`) uses `SqliteKbStore` — LLM output from the updated prompt is never validated
  - The `KbStoreError::Validation` variant is dead code in production
  - The `KbStore` trait has no `validation_mode` surface, so `SqliteKbStore` has no way to know which mode to use
  -> Fix: add a `validation_mode: ValidationMode` field to `SqliteKbStore` (constructor parameter), call `validate_body()` in `insert_key_block()` and `update_key_block()` before the SQL INSERT/UPDATE, and expose configuration from callers (`nexus42` CLI, `nexus-daemon-runtime`, `kb_extract_work`).

### 🟡 Warning
- **W-001: Advisory novel_category→block_type mapping documented but not implemented** (`crates/nexus-kb/src/validation.rs:119-123`)
  The comment says "Advisory: warn (via log, not error) if the novel_category doesn't map to the default block_type". However, the implementation only does `let _ = block_type;` — no logging occurs. The `nexus-kb` crate has no `tracing` or `log` dependency, so logging cannot be added without a new dependency. This means the advisory mapping from entity-scope-model.md §5.1.1 (e.g., `novel_category: "character"` with `block_type: "info_point"`) is not surfaced to users at all.
  -> Fix: either add `tracing` workspace dependency to `nexus-kb` and implement `tracing::warn!("novel_category '{}' does not map to default block_type {:?}", category, block_type)`, or remove the misleading comment and document the deferral explicitly.

- **W-002: SqliteKbStore block_type storage format fragility** (`crates/nexus-local-db/src/kb_store.rs:154-166`, `crates/nexus-local-db/src/kb_store.rs:190`)
  `SqliteKbStore` stores `block_type` as PascalCase via `format!("{:?}", kb.block_type)` (e.g., "Character") and parses it back via a manual `match` in `parse_block_type()`. The wire format and new prompt use snake_case (e.g., "character"). This is a pre-existing pattern, not introduced by this PR, but it creates fragility: if the `BlockType` `Debug` impl changes (e.g., a future `#[derive]` change), the database becomes unreadable. The validation module correctly uses the typed enum — the fragility is only in the SQLite persistence layer.
  -> Fix (deferred, not blocking): consider storing `block_type` as snake_case (matching wire format) via `serde_json::to_string(&kb.block_type)` or a dedicated `as_str()` method, and parsing via `serde_json::from_str`.

### 🟢 Suggestion
- **S-001: NOVEL_CATEGORIES duplicated between code and prompt** (`crates/nexus-kb/src/validation.rs:26-34`, `crates/nexus-orchestration/embedded-presets/kb-extract/prompts/extract.md:34`)
  The seven novel categories are hardcoded in both the Rust constant and the prompt template. If categories change, both must be updated. Consider a doc test or integration test that validates the prompt's category list against the Rust constant to catch drift.

- **S-002: Dead code `let _ = block_type;`** (`crates/nexus-kb/src/validation.rs:123`)
  The `block_type` parameter is bound to `_` with a comment about advisory mapping that is not implemented (see W-001). If the advisory mapping is deferred, remove the dead code and the misleading comment to avoid confusion. If it will be implemented soon, leave it but add a `// TODO(V1.40 P2): implement advisory logging` marker.

- **S-003: KbStore trait could expose validation mode** (`crates/nexus-kb/src/store.rs:79-119`)
  Currently `validation_mode` is an implementation detail of `InMemoryKbStore` only. The `KbStore` trait has no validation surface. If `SqliteKbStore` also needs validation (see C-001), consider adding a method like `fn set_validation_mode(&mut self, mode: ValidationMode)` to the trait, or making it a constructor parameter pattern that all implementors follow. This would make the validation contract explicit at the trait level rather than implicit per-implementation.

- **S-004: Test `test_invalid_block_type_via_deserialization` is misleading** (`crates/nexus-kb/src/store.rs:812-821`)
  The test name suggests it tests "invalid block_type fails with structured error" (AC1), but it only tests serde deserialization of raw JSON — it never exercises the store's validation path. The comment acknowledges this. Consider either: (a) renaming to `test_block_type_deserialization_rejects_unknown` to match what it actually tests, or (b) adding a test that exercises the store path with a valid `BlockType` but invalid body (which is already covered by other tests).

## Source Trace
- Finding ID: C-001
- Source Type: git-diff + manual-reasoning
- Source Reference: diff of `crates/nexus-local-db/src/kb_store.rs` (no changes in this PR — validation not wired)
- Confidence: High

- Finding ID: W-001
- Source Type: git-diff + manual-reasoning
- Source Reference: `crates/nexus-kb/src/validation.rs:119-123`, `crates/nexus-kb/Cargo.toml` (no tracing/log dep)
- Confidence: High

- Finding ID: W-002
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-local-db/src/kb_store.rs:154-166`, `crates/nexus-local-db/src/kb_store.rs:190`
- Confidence: Medium

- Finding ID: S-001
- Source Type: git-diff
- Source Reference: `crates/nexus-kb/src/validation.rs:26-34` vs `extract.md:34`
- Confidence: Medium

- Finding ID: S-002
- Source Type: git-diff
- Source Reference: `crates/nexus-kb/src/validation.rs:123`
- Confidence: High

- Finding ID: S-003
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-kb/src/store.rs:79-119` (KbStore trait), `crates/nexus-kb/src/store.rs:127-131` (InMemoryKbStore)
- Confidence: Medium

- Finding ID: S-004
- Source Type: git-diff
- Source Reference: `crates/nexus-kb/src/store.rs:812-821`
- Confidence: Low

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

**Rationale**: C-001 is blocking — the plan's T3 explicitly requires validation in `SqliteKbStore`, and the production path has zero body validation. W-001 (missing advisory logging) and W-002 (storage format fragility) are non-blocking but should be addressed. The validation module itself (`validation.rs`) is well-structured, follows crate conventions, correctly reuses `nexus_contracts::BlockType` as SSOT, and has thorough test coverage. The prompt update (`extract.md`) correctly uses wire snake_case values. Spec status headers are properly updated. The architecture is sound — the gap is purely in the production wiring.

## Revalidation

### Fix context

This targeted re-review covers the 6 fix commits (`2b0cf8fe..fbd301c4`) addressing the three blocking findings from the initial QC #1 review:

| Finding | Description | Status |
|---------|-------------|--------|
| C-001 | SqliteKbStore has no body validation | Resolved |
| W-001 | Advisory novel_category→block_type mapping not logged | Resolved |
| W-002 | SqliteKbStore block_type storage format fragility | Resolved |

### Diff since previous review

```
fbd301c4 docs(specs): QC1 C-001 / QC2 W2 — canonical_name grammar in entity-scope-model §5.1.1
9e2a5cbc fix(kb): kb_extract_work — parse structured body from LLM response
41c0d2d8 fix(kb): QC1 C-001 / QC2 C1 — wire validate_body into SqliteKbStore + QC1 W-002 — stable block_type storage
cf4bea22 fix(kb): wire canonical_name + body validation into InMemoryKbStore
61d2b060 fix(kb): QC1 W-001 / QC2 W1 — emit tracing::warn! on advisory mismatch + QC2 W2 — canonical_name validation
706fb605 refactor(kb): QC2 W3 — structured ValidationError with ValidationKind enum
```

9 files changed, 674 insertions, 45 deletions.

### Re-verification

**C-001 (SqliteKbStore validation) — RESOLVED**

- `SqliteKbStore` now has a `validation_mode: ValidationMode` field (line 104) with two constructors: `new()` (Generic default, line 112) and `with_validation_mode()` (line 121).
- `insert_key_block()` (lines 222-228) calls `validate_canonical_name()` then `validate_body()` before SQL INSERT.
- `update_key_block()` (lines 464-470) calls `validate_canonical_name()` then `validate_body()` before SQL UPDATE.
- `kb_extract_work.rs` (lines 316-325) now parses the LLM response body as structured JSON via `serde_json::from_str`, falling back to plain summary on parse failure.
- The `validation_err()` helper (lines 202-216) maps `nexus_kb::KbError` variants (including the new structured `ValidationError` with `ValidationKind`) into `KbStoreError::Validation`.
- Tests at lines 816-943 use `SqliteKbStore::with_validation_mode(pool, ValidationMode::Novel)` to exercise the novel validation path.
- **Evidence**: `cargo test -p nexus-local-db` — 163 passed, 0 failed. `cargo clippy -p nexus-local-db` — clean.

**W-001 (advisory logging) — RESOLVED**

- `validation.rs` (lines 222-236) now emits `tracing::warn!` when the `novel_category` does not map to the default `block_type`.
- The `nexus-kb` crate now depends on `tracing` (confirmed in `Cargo.toml`).
- The advisory message includes `novel_category`, `provided_block_type`, and `default_block_type` fields for structured log consumption.
- **Evidence**: `grep "tracing::warn!" crates/nexus-kb/src/validation.rs` confirms the single call site at line 225.

**W-002 (block_type storage format) — RESOLVED**

- `SqliteKbStore` now stores `block_type` as snake_case via `serde_json::to_string(&kb.block_type)` at all three call sites (lines 243, 478, 515).
- `parse_block_type()` (lines 177-193) accepts both snake_case (serde, primary path) and PascalCase (legacy fallback).
- The wire format, prompt, and storage now all use the same snake_case representation.
- **Evidence**: `grep "serde_json::to_string(&kb.block_type)" crates/nexus-local-db/src/kb_store.rs` confirms 3 call sites.

### Whole crate sanity

| Check | Result |
|-------|--------|
| `cargo build -p nexus-kb -p nexus-local-db -p nexus-orchestration --all-targets` | PASS (1 pre-existing warning in e2e_novel_writing) |
| `cargo test -p nexus-kb -p nexus-local-db -p nexus-orchestration` | PASS (all tests, 0 failures) |
| `cargo clippy -p nexus-kb -p nexus-local-db -p nexus-orchestration -- -D warnings` | PASS (clean) |
| `cargo +nightly fmt --all -- --check` | PASS (fmt_exit=0) |

### Updated verdict

**Approve** — all three blocking findings (C-001, W-001, W-002) are resolved. No new architecture-level findings. The four S-001..S-004 suggestions remain as non-blocking improvements for future iterations.

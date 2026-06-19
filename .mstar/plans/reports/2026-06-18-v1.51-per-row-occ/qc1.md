---
report_kind: qc_review
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-18-v1.51-per-row-occ
verdict: Approve
generated_at: 2026-06-19T18:00:00Z
---

# Code Review Report — QC #1 (Architecture / Maintainability)

## Reviewer Metadata

- **Reviewer**: @qc-specialist
- **Runtime Agent ID**: qc-specialist
- **Runtime Model**: deepseek/deepseek-v4-pro
- **Review Perspective**: Architecture coherence and maintainability risk
- **Report Timestamp**: 2026-06-19T18:00:00Z

## Scope

- **plan_id**: `2026-06-18-v1.51-per-row-occ`
- **Review range / Diff basis**: `iteration/v1.51...HEAD` (= `00829432...e988291a`)
- **Working branch (verified)**: `feature/v1.51-per-row-occ`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-b-p1`
- **Files reviewed**: 15 (5 new, 9 modified, 1 completion report)
- **Commit range**: `00829432` (PM clippy fix) → `e988291a` (completion report); 2 commits:
  - `f5eecf3d` feat(nexus-local-db,nexus42,nexus-orchestration): per-row OCC + CAS generalisation + E_VERSION code
  - `e988291a` docs(v1.51-t-b-p1): Completion Report v2 — per-row OCC + CAS generalisation
- **Lines**: +1479 / −40
- **Tools run**:
  - `cargo test -p nexus-local-db --test cas_migration_roundtrip` (5 passed)
  - `cargo test -p nexus42 --test cli_version_error` (4 passed)
  - `cargo test -p nexus42 --test kb_adopt_cas` (4 passed)
  - `cargo test -p nexus-orchestration --test cron_supervisor` (22 passed)
  - `cargo test -p nexus-local-db --test file_lock` (regression, 3 passed)
  - `cargo test -p nexus42 --test cli_lock_contention` (regression, 3 passed)
  - `cargo test -p nexus-orchestration --test novel_review_master review_master_llm` (T-A P0 regression, 2 passed)
  - `cargo test -p nexus-orchestration --lib -- llm` (T-A P0 regression, 50 passed)
  - `cargo clippy --all -- -D warnings` (**PASS** — 0 errors)
  - `cargo doc -p nexus-local-db --no-deps` (11 warnings, 4 in cas.rs — see W-001)

## Findings

### 🔴 Critical

None.

### 🟡 Warning

#### W-001 — Broken intra-doc link in `cas.rs` module header

- **File**: `crates/nexus-local-db/src/cas.rs`, line 7
- **Issue**: The module-level doc comment references `cas_update_result` but the actual exported function is `cas_check`. This produces a `rustdoc::broken_intra_doc_links` warning:
  ```
  warning: unresolved link to `cas_update_result`
  7 | //! - [`cas_update_result`] — check the result of a version-guarded UPDATE.
    |         ^^^^^^^^^^^^^^^^^ no item named `cas_update_result` in scope
  ```
- **Impact**: Developers reading the rendered module docs (or IDE hover) see a broken link. The documentation is misleading — the correct function name is `cas_check`.
- **Fix**: Change `cas_update_result` → `cas_check` on line 7.
- **Also**: Two additional doc warnings in the same file — `DEFAULT_MAX_ATTEMPTS` and `DEFAULT_BACKOFF_MS` are private `const` items linked from the public `with_cas_retry` doc comment. These don't produce broken links at runtime but violate the convention that public docs should not reference private items. Either make these `pub(crate)` or remove the doc links (see S-005).

### 🟢 Suggestion

#### S-001 — `concurrency.md` status header stale after T-B P1 extension

- **File**: `.mstar/knowledge/specs/concurrency.md`, line 3
- **Issue**: The header reads `**Status**: Draft (V1.51 T-B P0)` but T-B P1 added a major normative section (§7 Per-Row OCC, 80+ lines). The parenthetical version tag should reflect the latest active contributor.
- **Suggestion**: Update to `Draft (V1.51 T-B P1)` or `Draft (V1.51 T-B P0–P1)` to indicate the full extension scope. Per `specs/AGENTS.md`, drafts are "Free until iteration P5" so this is purely a documentation consistency issue.

#### S-002 — `CliError::VersionConflict.actual_version` always `None` in `kb_adopt`

- **File**: `crates/nexus42/src/commands/creator/world/kb.rs`, lines 538–545
- **Issue**: When `mark_confirmed_in_tx_with_cas` returns `VersionMismatch { actual: Some(v), .. }`, the `kb_adopt` error mapping creates `CliError::VersionConflict { actual_version: None, .. }` — discarding the actual version. The Display impl handles `None` gracefully (shows `?`) but the user loses diagnostic detail (e.g. "expected v0, actual v3" vs "expected v0, actual v?").
- **Suggestion**: Map `actual` from `VersionMismatch` into `actual_version`:
  ```rust
  CliError::VersionConflict {
      table: "kb_extract_jobs".to_string(),
      row_id: extract_job_id.to_string(),
      expected_version: candidate_version,
      actual_version: e.actual, // propagate the value
  }
  ```
  This requires destructuring the `VersionMismatch` variant in the match arm.

#### S-003 — `with_cas_retry` public API may be premature

- **File**: `crates/nexus-local-db/src/cas.rs`, `pub async fn with_cas_retry`
- **Issue**: The helper is `pub`, documented, and tested (2 tests), but no production caller uses it. The cron supervisor (`cron_supervisor.rs`) uses a manual retry loop because `enqueue_cron_schedule` returns `AutoChainError`, not `LocalDbError` — the helper's signature doesn't match the call-site. The completion report states this is "dormant until future T-A P1/P2 paths."
- **Suggestion**: Accept as-is (dormant public API is an intentional architectural choice), but note that a future caller may discover the signature mismatch pattern repeats. Consider either (a) a generic retry helper accepting `FnMut() -> Result<T, E>` where `E` can be pattern-matched, or (b) keep the manual loop pattern as the canonical approach and document when to use the helper vs manual loop.

#### S-004 — Test name `test_kb_adopt_stale_preimage_returns_version_conflict` is misleading

- **File**: `crates/nexus42/tests/kb_adopt_cas.rs`, line 62
- **Issue**: The test body acknowledges it cannot deterministically test the stale-preimage path through `kb_adopt` and delegates to unit tests. The test name suggests it verifies the named behavior, but the body merely documents why it can't. A reader scanning test names may incorrectly assume full-path coverage.
- **Suggestion**: Rename to `test_kb_adopt_with_consistent_version_succeeds` or split into a doc-comment note about the limitation.

#### S-005 — Private const links in public `with_cas_retry` doc comment

- **File**: `crates/nexus-local-db/src/cas.rs`, lines 138–143 (`DEFAULT_MAX_ATTEMPTS`, `DEFAULT_BACKOFF_MS`)
- **Issue**: Two `rustdoc` warnings about public documentation linking to private items. The `# Defaults` section of the `with_cas_retry` doc comment links `[`DEFAULT_MAX_ATTEMPTS`]` and `[`DEFAULT_BACKOFF_MS`]`, but both are `const` items in the module scope (not `pub`).
- **Suggestion**: Either make both constants `pub(crate)` (preferred, since they're documented as public API defaults) or replace the intra-doc links with plain text values (`3`, `100 ms`).

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|---|---|---|---|
| W-001 | rustdoc | `cargo doc -p nexus-local-db --no-deps` → `warning: unresolved link to cas_update_result` | High |
| S-001 | manual-reasoning | `git diff 00829432...e988291a -- .mstar/knowledge/specs/concurrency.md` — header line 3 unchanged | High |
| S-002 | manual-reasoning | `crates/nexus42/src/commands/creator/world/kb.rs:538-545` — `actual_version: None` hardcoded | High |
| S-003 | manual-reasoning | `cas.rs` helper vs `cron_supervisor.rs` manual loop — signature mismatch analysis | Medium |
| S-004 | manual-reasoning | `kb_adopt_cas.rs:62-106` — test body documents limitation of test | High |
| S-005 | rustdoc | `cargo doc -p nexus-local-db --no-deps` → `public documentation for with_cas_retry links to private item` | High |

## Summary

| Severity | Count |
|---|---|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 5 |

**Verdict**: **Approve**

The architecture is sound — the CAS generalisation cleanly abstracts the pattern from V1.50 P0 (`set_schedule_json_tx`), the acquire-order discipline (file lock → DB lock → CAS) is correctly documented and implemented, E_VERSION (76) is properly distinct from E_LOCK (75) and E_LOCK_IO (78), and all regression tests (T-B P0 advisory lock, T-A P0 LLM extraction, V1.50 cron) pass. The `#[allow(clippy::too_many_lines)]` annotations carry documented rationale combining both T-B P0 and T-B P1 contributions. Wire contracts are unchanged.

The single **Warning (W-001)** — a broken intra-doc link in the `cas.rs` module header — prevents `Approve`. It is a trivial one-character fix (`cas_update_result` → `cas_check`). No other blocking issues were found.

### Architecture assessment

| Concern | Verdict |
|---|---|
| CAS helper abstraction (reuse of V1.50 P0 pattern) | ✅ Clean generalisation |
| Acquire-order discipline (file lock → DB lock → CAS) | ✅ Correct; documented §7.5 |
| E_VERSION (76) distinct from E_LOCK (75) / E_LOCK_IO (78) | ✅ Stable CLI exit codes |
| T-B P0 advisory lock preservation | ✅ `file_lock` + `cli_lock_contention` tests pass |
| T-A P0 LLM extraction preservation | ✅ `review_master_llm_*` + 50 lib tests pass |
| Wire contract drift | ✅ No `schemas/` changes |
| `#[allow]` annotation justification | ✅ Both `kb_adopt` and `cron_supervisor` annotations cite T-B P0 + T-B P1 contributions |
| CAS retry on cron path (dormant) | ✅ Architecturally correct; documented as dormant |
| `novel_pool_entries` version column (unused) | ✅ Column exists; CAS call-sites deferred to V1.52+ (documented) |
| Documentation quality | ⚠️ W-001: broken doc link in module header |

## Revalidation (2026-06-19)

- **Resolved: W-001 (qc1, broken doc link)** — Fixed intra-doc link in `crates/nexus-local-db/src/cas.rs:7`: `[cas_update_result]` → `[cas_check]`, matching the actual function `pub fn cas_check(...)` at line 54. Commit `ef16f12f` renames the doc link; `grep -i cas_update_result` against `cargo doc -p nexus-local-db --no-deps` stderr confirms zero hits (EXIT 1 = no match).

- **Mechanism**: One-character edit in module-level doc comment — the reference `[`cas_update_result`]` was a typo for the actual public function `cas_check` in the same file. The fix aligns the intra-doc link with the exported symbol name. No other code was touched; no semantic change.

- **Evidence**:
  - Fix commit: `ef16f12f` — `fix(nexus-local-db): cargo doc unresolved link cas_update_result → cas_check (closes QC1 W-001)`
  - `cargo doc -p nexus-local-db --no-deps 2>&1 | grep -i cas_update_result` → no output (EXIT 1)
  - `cargo test -p nexus-local-db --test cas_migration_roundtrip` → 5 passed
  - `cargo test -p nexus-daemon-runtime --test cron_cas_retry` → 3 passed (new from qc2 W-002 fix)
  - `cargo test -p nexus42 --test cli_version_error` → 4 passed
  - `cargo test -p nexus42 --test kb_adopt_cas` → 6 passed
  - `cargo test -p nexus-local-db --test file_lock` → 3 passed (regression)
  - `cargo test -p nexus42 --test cli_lock_contention` → 3 passed (regression)
  - `cargo test -p nexus-orchestration --test cron_supervisor` → 22 passed (regression)
  - `cargo test -p nexus-orchestration -- llm_extract` → 15 passed (regression)
  - `cargo clippy --all -- -D warnings` → PASS
  - `cargo +nightly fmt --all --check` → PASS

- **Re-verdict**: Approve

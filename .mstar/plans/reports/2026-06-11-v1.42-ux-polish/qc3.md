---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-11-v1.42-ux-polish"
verdict: "Request Changes"
generated_at: "2026-06-12"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-12

## Scope
- plan_id: 2026-06-11-v1.42-ux-polish
- Review range / Diff basis: merge-base: 868f1b21 + tip: HEAD (ad180b44) — equivalent to `git diff 868f1b21...HEAD`
- Working branch (verified): HEAD (detached at ad180b44 on iteration/v1.42 integration line)
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-plast-qc
- Files reviewed: 8 changed files (229 insertions, 87 deletions)
- Commit range: d04ae9f4..ad180b44 (8 commits incl. merge)
- Tools run:
  - `cargo test -p nexus42 -- creator_works`
  - `cargo test -p nexus-orchestration -p nexus-daemon-runtime -p nexus42`
  - `cargo test -p nexus-daemon-runtime --test selection_pool`
  - `cargo test -p nexus-local-db`
  - `cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings`
  - `cargo +nightly fmt --all --check`

## Parallel Review — Performance / Reliability Lens

This review focuses on the P-last UX polish and residual triage through the performance/reliability lens: slug auto-suffix collision handling, CLI startup/list output cost, refactor impact, and hermetic test coverage of new behaviors.

## Findings

### 🔴 Critical
_None._

### 🟡 Warning

#### W-01: Inaccurate closure note for R-V141P1-12 in `status.json`

The `closure_note` for residual `R-V141P1-12` claims the CJK fallback now produces a **"romanized slug"** (e.g. `ling-gan-he-chuang-yi`), but the actual implementation in `crates/nexus-local-db/src/inspiration_items.rs::generate_fallback_slug()` returns `idea-<6-hex>` (a short timestamp-derived ID). The unit test `test_title_to_slug_chinese` asserts `slug.starts_with("idea-")`, confirming the implemented behavior is the hex fallback, not romanization.

- Impact: Future readers / downstream QA will misread the SSOT and expect romanized paths on disk; handoff reliability degrades.
- Fix: Update `.mstar/status.json` closure_note for `R-V141P1-12` to match the actual `idea-<hex>` fallback behavior.

#### W-02: Required scoped test command fails on pre-existing `works_api.rs` tests

`cargo test -p nexus-orchestration -p nexus-daemon-runtime -p nexus42` fails with two unrelated tests in `crates/nexus-daemon-runtime/tests/works_api.rs`:

- `handler_append_inspiration_returns_404_for_unknown` expects `404`, gets `500` (runtime-lock acquire path does not gracefully handle missing Work).
- `patch_work_stage_change_is_auditable` panics on `Locked { holder: "cli:http:..." }` (stale runtime lock from V1.42 P0).

Neither failing test nor the code paths they exercise were modified in the P-last diff (works.rs diff is limited to DTO field additions). Both failures are consistent with V1.42 P0 runtime-lock behavior and reproduce in isolation.

- Impact: The evidence command required by the assignment cannot be reported green; a future CI gate running the same scoped test command will fail.
- Fix: Out of P-last scope. Recommend PM/QA verify against base commit `868f1b21` and, if reproduced, open/track as a residual under the originating plan (V1.42 runtime-lock / V1.42 P0). Do not block P-last on this, but do not silently omit it from the report.

### 🟢 Suggestion

#### S-01: Synchronous filesystem probes in slug collision loop

`create_inspiration_with_scaffold()` performs the collision-detection loop (`ap.exists()`) before entering `tokio::task::spawn_blocking`. The loop is capped at 100 iterations and only runs at inspiration creation time, so it is not a hot path. However, moving the entire candidate-slug resolution (file-exists checks + tmp+rename) into `spawn_blocking` would keep the async runtime unblocked and align with the function's own comment about blocking I/O.

- Fix: Compute `rel_path` / `abs_path` inside the `spawn_blocking` closure, or at least perform the `exists()` checks there.

#### S-02: No integration-level test for CJK / fallback slug behavior

`selection_pool.rs` has a strong test (`test_inspiration_add_auto_suffixes_on_collision`) for the English collision path, and `nexus-local-db` has a unit test for `title_to_slug` with CJK. There is no end-to-end test that a pure-CJK title flows through the daemon handler and produces a valid `idea-<hex>` path. Adding one would prevent future regressions if the fallback strategy changes.

- Fix: Add a `selection_pool.rs` TC that adds an inspiration with a pure-CJK title and asserts the response `rel_path` starts with `Pool/Ideas/idea-`.

#### S-03: `R-V141P1-14` residual state inconsistency

In `.mstar/status.json`, `R-V141P1-14` has `decision: "accept"` but `lifecycle: "open"` with a "waived" closure note. Accepted/waived residuals should normally be `lifecycle: "resolved"` (or remain `defer` if intentionally deferred). This is metadata hygiene, not a code risk.

- Fix: PM should reconcile the `decision` / `lifecycle` fields for `R-V141P1-14` when closing out T5.

## Source Trace

- **W-01**
  - Source Type: doc-rule / manual-reasoning
  - Source Reference: `.mstar/status.json` R-V141P1-12 closure_note; `crates/nexus-local-db/src/inspiration_items.rs` lines 510-519 (`generate_fallback_slug`)
  - Confidence: High

- **W-02**
  - Source Type: static-analysis / test failure
  - Source Reference: `cargo test -p nexus-orchestration -p nexus-daemon-runtime -p nexus42`; `crates/nexus-daemon-runtime/tests/works_api.rs:454` and `:1030`
  - Confidence: High

- **S-01**
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus-local-db/src/inspiration_items.rs` lines 180-204
  - Confidence: Medium

- **S-02**
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus-daemon-runtime/tests/selection_pool.rs` TC6; `crates/nexus-local-db/src/inspiration_items.rs` lines 531-543
  - Confidence: Medium

- **S-03**
  - Source Type: manual-reasoning
  - Source Reference: `.mstar/status.json` R-V141P1-14
  - Confidence: High

## Performance / Reliability Assessment

| Area | Assessment |
|------|------------|
| Slug collision handling | Linear scan capped at 100; acceptable for local-first, non-hot path. File-exists checks could move into `spawn_blocking` (S-01). |
| CLI startup / status cost | New `completion_locked_at` field access and lock-icon formatting add negligible overhead. `print_completion_lock_hint` performs config load + one `exists()` call only when the Work is locked; bounded. |
| List output cost | One additional JSON field access per row; negligible. |
| Refactor impact (`handle_status` dedup) | Extraction of `print_chapter_table` and `truncate_with_ellipsis` is behavior-preserving; no perf regression. |
| Hermetic test coverage | `selection_pool.rs` covers auto-suffix collision and cross-creator guards. `nexus-local-db` unit tests cover CJK fallback. Missing end-to-end CJK test (S-02). |
| Memory / resource safety | `create_inspiration_with_scaffold` rolls back the MD file on DB failure; good. `RuntimeLockGuard` release is unchanged. |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

Rationale: W-01 is inside P-last scope (T5 residual closeout) and trivial to fix — the SSOT closure note must match implemented behavior. Once W-01 is corrected, the remaining items (W-02 pre-existing test failures, S-01-S-03) can be accepted as residuals or suggestions without blocking P-last merge.

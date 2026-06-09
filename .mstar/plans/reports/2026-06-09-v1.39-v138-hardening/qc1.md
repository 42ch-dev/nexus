---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-09-v1.39-v138-hardening"
verdict: "Approve"
generated_at: "2026-06-09"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-09T10:30:00+08:00

## Scope
- plan_id: 2026-06-09-v1.39-v138-hardening
- Review range / Diff basis: merge-base: 1b68d6ca (iteration/v1.39 HEAD with P0 + P0.5 closed) + tip: 24919b27 (feature/v1.39-v138-hardening HEAD); equivalent to git diff 1b68d6ca...24919b27
- Working branch (verified): feature/v1.39-v138-hardening
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.39-p5
- Files reviewed: 5
- Commit range: 932097ea..24919b27 (4 commits)
- Tools run: cargo clippy, cargo test (4 suites), cargo +nightly fmt --check, git diff, manual code review

## Findings

### 🔴 Critical
(None)

### 🟡 Warning

#### W-001: No guard registry — stage_advance guards are inline call sites
- **Location**: `crates/nexus42/src/commands/creator/run.rs:1165-1175`
- **Observation**: `validate_produce_chapter_context` and `reject_produce_when_novel_complete` are called as sequential inline `?` expressions in `stage_advance()`. With only 2 guards this is fine, but as more stage-specific guards accumulate (e.g., research-stage gate, review-stage gate), the inline pattern will make `stage_advance()` hard to reason about.
- **Fix**: Not blocking for P5. In V1.40, consider extracting a `StageAdvanceGuards` registry (e.g., `Vec<fn(&StageContext) -> Result<()>>`) or a `check_stage_advance_guards()` facade that runs all registered guards in order. This would make the guard set discoverable and testable as a unit.
- **Severity**: Warning (maintainability risk, not correctness)

#### W-002: R-V138P0-02 / R-V138P0-04 / R-V138P1-04 accept rationales are sound but lack explicit V1.40 tracking
- **Location**: Residuals in `status.json` under `2026-06-08-v1.38-multi-chapter-selection-status` and `2026-06-08-v1.38-novel-writing-parameterization`
- **Observation**: Three residuals are accepted as out-of-P5-scope:
  - R-V138P0-02 (CLI missing-file hints): accept, DB SSOT preserved, reconcile-chapters covers remediation.
  - R-V138P0-04 (chapters uncapped): accept, typical novel scales <100 chapters, local DB not exposed to untrusted clients.
  - R-V138P1-04 (template required without defaults): accept, current callers all populate via stage_advance.
  
  All three rationales are sound. However, the plan does not specify whether they should be tracked as new V1.40 residuals or closed entirely. R-V138P0-02 and R-V138P1-04 are actionable improvements (missing-file hints, runtime validation for template vars) that should be tracked for V1.40; R-V138P0-04 is genuinely backlog material.
- **Fix**: PM should decide in T5 (status.json update): (a) close R-V138P0-04 as accept/backlog, (b) re-target R-V138P0-02 and R-V138P1-04 to V1.40 with updated `decision: defer` and `target_date: V1.40`.
- **Severity**: Warning (tracking gap, not correctness)

### 🟢 Suggestion

#### S-001: `reject_produce_when_novel_complete` error message could include chapter counts
- **Location**: `crates/nexus42/src/commands/creator/run.rs:1016-1023`
- **Observation**: The error message says "no remaining active chapter" but doesn't tell the user how many chapters are finalized or what the total was. Adding `total_planned_chapters` and `finalized_count` to the error would make it more informative for debugging.
- **Suggestion**: Consider enriching the error with chapter counts (requires passing them from the daemon response). Non-blocking; the current hint ("advance to persist" + status command) is already actionable.

#### S-002: `test_is_work_completed_false_when_total_planned_chapters_null` uses raw SQL UPDATE instead of the crate's own `patch_work`
- **Location**: `crates/nexus-local-db/src/work_chapters.rs:970-988`
- **Observation**: The test uses `sqlx::query("UPDATE works SET ...")` directly rather than `works::patch_work()`. This bypasses the crate's own API and means the test won't catch regressions in `patch_work`'s behavior. However, the test's purpose is to verify `is_work_completed` logic, not `patch_work`, so this is acceptable.
- **Suggestion**: Consider adding a comment explaining why raw SQL is used here (to set up a specific DB state without going through the full patch_work validation path). Non-blocking.

#### S-003: Idempotency test uses 3 GETs but only 2 are needed to prove idempotency
- **Location**: `crates/nexus-daemon-runtime/tests/works_api.rs:1298-1314`
- **Observation**: The test does 3 GETs (first → verify promotion, second → verify idempotent, third → defense-in-depth). Two GETs would be sufficient to prove idempotency; the third is harmless but adds noise.
- **Suggestion**: Keep the third GET (defense-in-depth is a valid testing strategy), but add a brief comment explaining it's a belt-and-suspenders check. Non-blocking.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|-----------|-------------|------------------|------------|
| W-001 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs:1165-1175` (inline guard calls) | Medium |
| W-002 | manual-reasoning | `status.json` residuals R-V138P0-02, R-V138P0-04, R-V138P1-04 | High |
| S-001 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs:1016-1023` (error message) | Low |
| S-002 | manual-reasoning | `crates/nexus-local-db/src/work_chapters.rs:970-988` (raw SQL in test) | Low |
| S-003 | manual-reasoning | `crates/nexus-daemon-runtime/tests/works_api.rs:1298-1314` (3rd GET) | Low |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Architecture Observations (Top 3)

1. **Triage decisions are sound across all 7 residuals.** The fix/accept calls are well-reasoned:
   - R-V138P0-01 (race window): accept-with-doc — the `next_chapter()` doc comment captures the single-writer invariant, the conditions that would invalidate it, and the exact upgrade path (atomic `UPDATE ... RETURNING chapter`). This is an exemplary accept-with-doc.
   - R-V138P0-03 (write-on-read): accept-with-doc + idempotency test — the `get_work()` doc explains *why* write-on-read is intentional (no daemon-side scheduler, platform needs canonical signal, idempotent), not just *what* it does. The idempotency test is robust (3 GETs, verifies `updated_at` unchanged on re-reads).
   - R-V138P0-05 (NULL test): fix — 2 tests at the right level (lib unit), covering both NULL and 0 edge cases. The NULL test goes further by verifying the guard holds even after intake completes.
   - R-V138P1-01 (empty-chapter schedule): fix — guard is correctly wired before `build_schedule_for_stage`, error is actionable (NOVEL_COMPLETE tag + persist hint + status command), 3 tests cover error/allow/skip cases.
   - R-V138P0-02, R-V138P0-04, R-V138P1-04: accept rationales are sound. See W-002 for tracking recommendation.

2. **The `reject_produce_when_novel_complete` guard is well-shaped and correctly placed.** Signature takes only what it needs (`target_stage`, `next_chapter`, `work_id`), returns the project's standard `Result<()>`, and is called after chapter context extraction but before `build_schedule_for_stage`. This is compatible with P0.5's `build_schedule_for_stage("produce", ...)` — the guard prevents the schedule from being built when `next_chapter` is `None`, so P0.5's produce path is only reached with a valid chapter. The guard pattern (early-return `Result<()>`) is consistent with `validate_produce_chapter_context` and the existing `check_stage_advance` gate.

3. **Doc comments are intent-explaining, not code-restating.** Both `next_chapter()` and `get_work()` docs explain architectural rationale (single-writer invariant, no daemon-side scheduler, platform needs canonical signal), failure semantics (log warning + return un-promoted), and future cleanup paths (atomic claim helper, post-finalize hook). This is the standard the codebase should hold for all accept-with-doc decisions.

## Cross-Plan Compatibility

- **P5 × P0.5**: Verified compatible. P5's `reject_produce_when_novel_complete` fires at line 1175, before `build_schedule_for_stage` is called at ~line 1200+. When `next_chapter` is `None`, the guard returns an error and `build_schedule_for_stage` is never reached. When `next_chapter` is `Some(...)`, the guard passes and P0.5's `build_schedule_for_stage("produce", ...)` runs normally with valid chapter context. No conflict.

## Regression Verification

| Suite | Result |
|-------|--------|
| `cargo clippy -p nexus42 -p nexus-local-db -p nexus-daemon-runtime -- -D warnings` | Clean (0 warnings) |
| `cargo +nightly fmt --all -- --check` | Clean (no diff) |
| `cargo test -p nexus-local-db --lib test_is_work_completed` | 4/4 passed |
| `cargo test -p nexus42 --lib -- reject_produce_when_novel_complete` | 3/3 passed |
| `cargo test -p nexus-daemon-runtime --test works_api handler_get_work_lazy_promotes` | 1/1 passed |
| `cargo test -p nexus-orchestration --test auto_chain` (P0 regression) | 21/21 passed |
| `cargo test -p nexus-orchestration --lib -- research` (P0.5 regression) | 8/8 passed |

## Branch Discipline

- ✅ Working branch: `feature/v1.39-v138-hardening` (matches Assignment)
- ✅ Review cwd: `.worktrees/v1.39-p5` (matches Assignment)
- ✅ Diff scope: 4 commits, 5 files, +317/-9 — only touches V1.38 residual scope (no P0/P0.5/P1..P4 scope creep)
- ✅ Zero source-file modifications by this reviewer
- ✅ Zero subagent invocations

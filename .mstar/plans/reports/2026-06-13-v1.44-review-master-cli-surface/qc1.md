---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-13-v1.44-review-master-cli-surface"
verdict: "Approve"
generated_at: "2026-06-13"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-13T12:30:00+08:00

## Scope (re-review)
- plan_id: 2026-06-13-v1.44-review-master-cli-surface
- Review range / Diff basis: c54b1aa6..a9262c33
- Working branch (verified): iteration/v1.44
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 3 (2 spec files + 1 code file; fix-wave delta only)
- Commit range: c54b1aa6..a9262c33 (2 fix commits + 1 fix-merge)
- Tools run: cargo clippy -p nexus42 -- -D warnings, cargo test -p nexus42 --test review_master_cli, cargo test -p nexus42 --test integration, cargo +nightly fmt --all --check

## Findings

### 🔴 Critical
(None)

### 🟡 Warning

#### W-1: `cli-command-ia.md` not updated despite plan T5 listing it — ✅ RESOLVED

**Original**: Plan T5 explicitly lists `cli-command-ia.md` as a target for spec amendments alongside `cli-spec.md`, `novel-writing/workflow-profile.md`, and `novel-writing/quality-loop.md`. However, `cli-command-ia.md` had zero changes in the original diff range — it did not mention `review-master` or `audit-chapter` at all.

**Resolution** (commit `9e953abd`): Added a `creator run` subcommands table to `cli-command-ia.md` §3.1 documenting both `review-master` and `audit-chapter` with role, shipped version, and cross-references to `novel-writing/quality-loop.md` and `novel-writing/manuscript-audit.md`.

#### W-2: `novel-writing/author-experience.md` not updated despite being a plan primary spec — ✅ RESOLVED

**Original**: The plan listed `novel-writing/author-experience.md` as a primary spec, but the file had zero changes and did not mention `review-master`.

**Resolution** (commit `9e953abd`): Added a "How do I run master-decision review?" row to the author questions table in `novel-writing/author-experience.md` §4, with cross-reference to `novel-writing/quality-loop.md` §3.4.

#### W-3: Duplicate Work fetch when both `--finding-id` and `--auto-schedule` are used — ✅ RESOLVED

**Original**: `handle_review_master` fetched the Work from the daemon API twice when both flags were active.

**Resolution** (commit `a5a9bd7e`, R-V144P1-005): Extracted `fetch_work_context` helper function that fetches the Work once and returns `(work_ref, topic, world_id, work)`. Both `--finding-id` and `--auto-schedule` paths now call this shared helper. The helper is well-documented with a doc comment explaining its purpose.

#### W-4: `--auto-schedule` uses global stale count but work-scoped `master_findings` — ✅ RESOLVED

**Original**: The `--auto-schedule` path queried the global `/v1/local/findings/stale` endpoint but enqueued a work-scoped schedule.

**Resolution** (commit `a5a9bd7e`, R-V144P1-003): The stale check is now scoped to the current Work — the code filters the `/stale` response client-side to count only findings where `work_id` matches the supplied `work_id`. The CLI help text was updated to document the work-scoped behavior. The `stale_count` in JSON output now reflects `work_stale_count` instead of the global count. User-facing messages were updated accordingly ("No stale findings for this Work", "stale finding(s) in this Work").

### 🟢 Suggestion

#### S-1: Extract enqueue helpers to reduce `handle_review_master` size — PARTIALLY ADDRESSED

**Original**: The function was ~310 lines with a `#[allow(clippy::too_many_lines)]` suppression.

**Status**: The fix wave extracted `fetch_work_context` (addressing W-3), but did not extract the `--finding-id` and `--auto-schedule` enqueue logic into separate private helper functions. The `#[allow(clippy::too_many_lines)]` suppression remains. This is acceptable as a deferred improvement — the duplication concern (W-3) is resolved, and the remaining function length is manageable.

#### S-2: `open_findings` serialized as JSON string in schedule input — UNCHANGED

**Status**: No change in the fix wave. This is a low-priority design note; the double-serialization pattern is intentional (passing structured data through a string-typed field) and the fix wave did not address it. Acceptable as deferred.

#### S-3: Hardcoded API path strings — UNCHANGED

**Status**: No change in the fix wave. Acceptable as deferred — the paths are used in only one function and centralization would be a separate refactoring effort.

#### S-4: `body_path` asymmetry between `--finding-id` and `--auto-schedule` — UNCHANGED

**Status**: No change in the fix wave. The asymmetry is intentional (auto-schedule covers all findings, not one chapter) and the fix wave did not add documentation. Acceptable as deferred.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | git-diff (re-review) | `git diff c54b1aa6..a9262c33 -- .mstar/knowledge/specs/cli-command-ia.md` — 9 lines added | High |
| W-2 | git-diff (re-review) | `git diff c54b1aa6..a9262c33 -- .mstar/knowledge/specs/novel-writing/author-experience.md` — 1 line added | High |
| W-3 | git-diff (re-review) | `git diff c54b1aa6..a9262c33 -- crates/nexus42/src/commands/creator/run.rs` — `fetch_work_context` helper extracted | High |
| W-4 | git-diff (re-review) | `git diff c54b1aa6..a9262c33 -- crates/nexus42/src/commands/creator/run.rs` — work-scoped stale filter | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 (all 4 resolved) |
| 🟢 Suggestion | 4 (unchanged; deferred) |

**Verdict**: Approve

## Revalidation

### Re-review scope

- **Fix-wave range**: `c54b1aa6..a9262c33` (2 fix commits + 1 fix-merge)
- **Fix commits**:
  - `9e953abd` — R-V144P1-001/002: spec updates for `cli-command-ia.md` and `novel-writing/author-experience.md`
  - `a5a9bd7e` — R-V144P1-003/004/005/006: code fixes for stale scoping, target_executor check, shared helper, limit raise
- **Re-review date**: 2026-06-13

### Per-finding disposition

| Finding | Fix Commit | Resolution | Status |
|---------|-----------|------------|--------|
| W-1 (cli-command-ia.md) | `9e953abd` | Added `creator run` subcommands table to §3.1 | ✅ RESOLVED |
| W-2 (novel-writing/author-experience.md) | `9e953abd` | Added "How do I run master-decision review?" row to §4 | ✅ RESOLVED |
| W-3 (Duplicate Work fetch) | `a5a9bd7e` | Extracted `fetch_work_context` shared helper | ✅ RESOLVED |
| W-4 (Global stale count) | `a5a9bd7e` | Client-side filter to work-scoped stale count | ✅ RESOLVED |

### Cross-cut fixes observed (not in original W-1..W-4 scope)

The fix commit `a5a9bd7e` also includes:
- **R-V144P1-004**: `target_executor` assertion before `--finding-id` enqueue — rejects non-master-targeted findings with a clear `CliError::Config` error. Good defensive check.
- **R-V144P1-006**: Findings list limit raised from 50→200 with a doc comment documenting the truncation risk. Acceptable as a pragmatic cap; daemon-side `target_executor` filter is deferred.

### Evidence (re-review)

- `cargo clippy -p nexus42 -- -D warnings` — **PASS** (clean)
- `cargo test -p nexus42 --test review_master_cli` — **PASS** (5/5)
- `cargo test -p nexus42 --test integration` — **PASS** (50/50)
- `cargo +nightly fmt --all --check` — **PASS** (no diffs)
- `git log --oneline c54b1aa6..a9262c33` — 2 fix commits + 1 fix-merge, as expected

### Architecture Coherence Assessment (updated)

The fix wave cleanly addresses all four Warnings without introducing new coupling or complexity. The `fetch_work_context` helper is well-scoped (single responsibility, clear return type). The work-scoped stale filter is implemented as a client-side filter on the existing `/stale` endpoint — a pragmatic choice that avoids a new daemon endpoint while correctly scoping the behavior. The spec updates in `cli-command-ia.md` and `novel-writing/author-experience.md` are minimal and well-placed — the IA table uses the same format as existing tables, and the author-experience row follows the established question-answer pattern.

### Maintainability Risk (updated)

All four maintainability concerns from the initial review are resolved. The remaining Suggestions (S-1 through S-4) are deferred improvements that do not block merge. The code is now in a maintainable state for the V1.44 release.

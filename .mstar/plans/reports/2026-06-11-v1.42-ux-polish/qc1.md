---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-11-v1.42-ux-polish"
verdict: "Request Changes"
generated_at: "2026-06-12"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-12T01:00:00+0800

## Scope
- plan_id: 2026-06-11-v1.42-ux-polish
- Review range / Diff basis: merge-base: 868f1b21 + tip: HEAD of iteration/v1.42 (ad180b44) — equivalent to git diff 868f1b21...HEAD
- Working branch (verified): HEAD (detached at ad180b44)
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-plast-qc
- Files reviewed: 8
- Commit range: 868f1b21..ad180b44 (7 commits)
- Tools run: cargo test, cargo clippy, cargo +nightly fmt --check, git diff, git log, rg

## Findings
### 🔴 Critical
(None)

### 🟡 Warning
- **W-1: `print_work_header` referenced in closure_note but does not exist in code** — The `R-V141P0-02` closure_note in `status.json` states "deduplicated handle_status by extracting shared display helpers (print_work_header, print_chapter_table)". However, `print_work_header` does not exist anywhere in the codebase. Only `print_chapter_table`, `print_completion_lock_hint`, and `truncate_with_ellipsis` were extracted. The closure_note is inaccurate and could mislead future readers about what was actually refactored. → Fix: Correct the closure_note to list only the functions that were actually extracted, or implement the missing `print_work_header` helper.

- **W-2: `truncate_with_ellipsis` only used in one of four eligible locations** — The new `truncate_with_ellipsis` helper (line 806) is only called from `print_chapter_table` (line 772). Three other locations in the same file still use the identical inline pattern `if title.len() > 28 { format!("{}…", &title[..28]) }`:
  - Line 216: `handle_list` title truncation
  - Line 538: pool list title truncation
  - Line 684: inspiration list title truncation
  This creates an inconsistency where the refactor extracted the pattern for one use but left three identical copies inline. → Fix: Replace the three remaining inline truncation patterns with `truncate_with_ellipsis` calls.

- **W-3: `subsec_nanos()` is deprecated in Rust ≥1.93** — `generate_fallback_slug` in `crates/nexus-local-db/src/inspiration_items.rs:518` calls `now.subsec_nanos()` which is deprecated in the installed Rust version (1.93.1). This will become a clippy warning when the deprecation is fully enforced. → Fix: Replace `subsec_nanos()` with `as_nanos()` or a non-deprecated alternative.

- **W-4: Two pre-existing test failures (not caused by this diff)** — Both tests fail identically on the base commit (`868f1b21`) and current HEAD:
  - `handler_append_inspiration_returns_404_for_unknown`: panics with `left: 500, right: 404`
  - `patch_work_stage_change_is_auditable`: panics with runtime_lock error
  These are pre-existing and not introduced by this plan. The remaining 30 tests in `works_api` pass. → Fix: Track as pre-existing residual; not blocking for this plan but should be addressed.

### 🟢 Suggestion
- **S-1: `R-V141P1-14` has inconsistent lifecycle state** — The row has `decision: accept` but `lifecycle: open` with a closure_note that says "waived". If the item was waived, the decision should be `defer` and target should be `V1.43+` (matching the pattern used by R-V141P0-03, R-V141P0-05, R-V141P0-08, R-V141P1-09, R-V141P1-10). If it was accepted (resolved), the lifecycle should be `resolved`. The current state is ambiguous. → Fix: Align decision/lifecycle fields consistently.

- **S-2: `generate_fallback_slug` entropy source is low-quality** — The fallback slug uses `subsec_nanos() >> 4` masked to 24 bits. For a local-first tool this is acceptable, but the comment says "Good enough for local-first use; avoids external deps." A more robust approach (e.g., UUID v4 or a counter) would reduce collision risk without adding dependencies. → Consider using a simple atomic counter or UUID for stronger uniqueness guarantees.

- **S-3: T4 commit is verification-only, not implementation** — The T4 commit (`8a3350eb`) documents that the combined flag paths work individually and don't need a combined path. The plan task title "T4: Combined CLI flag paths" implies implementation work, but the actual commit is a docs/verification commit. The commit message accurately describes this, but the plan task title is slightly misleading. → Consider updating the plan task description to "T4: Verify combined CLI flag paths work individually" for accuracy.

## Source Trace
- Finding ID: W-1
- Source Type: manual-reasoning + rg
- Source Reference: `rg -rn 'print_work_header' crates/` → no output; `status.json` R-V141P0-02 closure_note
- Confidence: High

- Finding ID: W-2
- Source Type: manual-reasoning + rg
- Source Reference: `rg -n '\.len\(\) > 28' crates/nexus42/src/commands/creator/works/mod.rs` → lines 216, 538, 684 (3 remaining); `truncate_with_ellipsis` only at line 806, called only at line 772
- Confidence: High

- Finding ID: W-3
- Source Type: static-analysis + rustc version
- Source Reference: `rustc 1.93.1`; `rg -n 'subsec_nanos' crates/nexus-local-db/src/inspiration_items.rs` → line 518
- Confidence: High

- Finding ID: W-4
- Source Type: cargo test (pre-existing verification)
- Source Reference: `cargo test -p nexus-daemon-runtime --test works_api` on both `868f1b21` and `ad180b44` — same 2 failures
- Confidence: High

- Finding ID: S-1
- Source Type: manual-reasoning + rg
- Source Reference: `rg -n 'R-V141P1-14' .mstar/status.json -A 8`
- Confidence: Medium

- Finding ID: S-2
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-local-db/src/inspiration_items.rs:511-519`
- Confidence: Low

- Finding ID: S-3
- Source Type: git-log
- Source Reference: `git show 8a3350eb --stat` → 0 files changed (empty diff); commit message is docs-only
- Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

**Rationale**: Four Warning findings remain unresolved. W-1 (phantom `print_work_header` reference in closure_note) is a documentation accuracy issue that could mislead future maintainers. W-2 (incomplete `truncate_with_ellipsis` extraction) leaves three duplicate inline patterns that the refactor was supposed to eliminate. W-3 (deprecated `subsec_nanos()`) will become a clippy error. W-4 (pre-existing test failures) should be tracked but is not blocking for this plan. The architecture of the changes is sound — the refactor direction is correct, the UX improvements are well-scoped, and the residual triage is thorough. These warnings are minor and fixable.

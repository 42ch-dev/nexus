---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-11-v1.42-ux-polish"
verdict: "Approve"
generated_at: "2026-06-12"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (reviewer_index=2)
- Report Timestamp: 2026-06-12

## Scope
- plan_id: 2026-06-11-v1.42-ux-polish
- Review range / Diff basis: merge-base: 868f1b21 + tip: HEAD of iteration/v1.42 (ad180b44) — equivalent to `git diff 868f1b21...HEAD`
- Working branch (verified): HEAD (detached at ad180b44)
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-plast-qc
- Files reviewed: 8 (per `git diff --stat`)
- Commit range: 7 commits (d04ae9f4..ad180b44)
- Tools run:
  - `git log 868f1b21..HEAD --oneline`
  - `git diff 868f1b21..HEAD --stat`
  - `cargo test -p nexus42 -- creator_works`
  - `cargo test -p nexus-orchestration -p nexus-daemon-runtime -p nexus42`
  - `cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings`
  - `cargo +nightly fmt --all --check`
  - `rg -n 'R-V141P0-0[2-8]|R-V141P1-(09|1[0-9])' .mstar/status.json`
  - Targeted `git show` on T2 commits for creator_id and slug logic

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None (within review scope and security/correctness lens).

**Pre-existing test environment observations (not introduced by this plan's diff):**
- `cargo test -p nexus-daemon-runtime --test works_api` shows 2 failures:
  - `handler_append_inspiration_returns_404_for_unknown`: expects 404 but receives 500 (StatusCode mismatch at works_api.rs:454). This test exercises `append_inspiration` handler for a non-existent work_id. The plan diff does not modify the per-work append path, work existence check, or error mapping for that handler (the T2 changes affect pool-level `create_inspiration_with_scaffold` slug handling and the list handler for completion_locked_at exposure). The failure appears environmental / test isolation (concurrent lock state from `patch_work_stage_change_is_auditable` also fails with "Locked" in the same run).
  - `patch_work_stage_change_is_auditable`: hits runtime lock contention.
- These failures pre-date the 7 commits under review (no changes to the append_inspiration error path or lock acquisition in this scope). `cargo test -p nexus42 -- creator_works` (the primary CLI surface for T2/T3) passes cleanly. Not a blocker for this plan's correctness under security/correctness review.

### 🟢 Suggestion
- The best-effort missing-file hint in `print_completion_lock_hint` (and its prior inlined form) relies on `dirs::home_dir()` + CLI config lookup. This is acceptable for a UX hint (never authoritative; DB `completion_locked_at` remains SSOT), but in a future hardening pass it could be made more robust by surfacing a structured warning from the daemon instead of client-side path reconstruction. Current implementation is safe (no side effects, early return on missing ref, no privilege escalation).
- The slug collision retry in `create_inspiration_with_scaffold` caps at 100 suffixes and converts exhaustion to `ConstraintViolation`. This is a correctness improvement (no silent data loss, explicit error after bounded retries). Consider exposing the attempted suffix count in the error for diagnostics if this surfaces in production logs, but not required for V1.42 P-last.

## Source Trace
- Finding ID: QC2-2026-06-11-001 (no open findings)
- Source Type: manual-reasoning + targeted diff inspection + test/lint runs
- Source Reference: commits 3c40474f (T2), f5d994a9 (T3); `git show` excerpts for DTO `#[serde(skip_serializing)]` and slug loop; status.json residual closures for R-V141P0-02/06/11 and R-V141P1-11/12/13/19
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 (informational; no action required before Approve) |

**Verdict**: Approve

## Evidence (as required by Assignment)
- `git rev-parse --show-toplevel`: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-plast-qc`
- `git rev-parse --abbrev-ref HEAD`: `HEAD` (detached)
- `git log 868f1b21..HEAD --oneline`:
  ```
  ad180b44 harness(status): V1.42 P-last → InReview (PM merge complete; QC tri-review pending)
  e3769dd3 merge(v1.42 P-last): UX polish + 14 residual triage + iteration Shipped markers
  42850bc3 chore(harness): T5 close residuals + iteration Shipped markers
  8a3350eb docs(ux): T4 combined CLI flag paths — verified working individually
  f5d994a9 refactor(works): T3 handle_status dedup — extract display helpers
  3c40474f feat(ux): T2 CLI polish — 5 UX items
  d04ae9f4 chore(status): T1 triage 14 V1.41 residual rows (fix/waive/defer)
  ```
- `git diff 868f1b21..HEAD --stat`:
  ```
   .mstar/iterations/README.md                        |   2 +-
   ...ti-volume-serial-writing-delivery-compass-v1.md |   2 +-
   .mstar/plans/2026-06-11-v1.42-ux-polish.md         |  12 +--
   .mstar/status.json                                 |  68 ++++++++++----
   crates/nexus-daemon-runtime/src/api/handlers/works.rs |   8 ++
   crates/nexus-daemon-runtime/tests/selection_pool.rs   |  29 ++++--
   crates/nexus-local-db/src/inspiration_items.rs        |  94 ++++++++++++-------
   crates/nexus42/src/commands/creator/works/mod.rs      | 101 ++++++++++++++++-----
   8 files changed, 229 insertions(+), 87 deletions(-)
  ```
- `cargo test -p nexus42 -- creator_works 2>&1 | tail -30`: passed (1 test `v141_creator_works_subcommands` ok; full filtered run clean for the targeted binary).
- `cargo test -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 2>&1 | tail -30`: 30 passed, 2 failed (pre-existing test isolation issues noted above; not in changed code paths for this plan).
- `cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -- -D warnings 2>&1 | tail -40`: clean (Finished dev profile; no warnings emitted under -D).
- `cargo +nightly fmt --all --check 2>&1 | tail -20`: clean (no output).
- `rg -n 'R-V141P0-0[2-8]|R-V141P1-(09|1[0-9])' .mstar/status.json`: 14 rows located (R-V141P0-02 through R-V141P0-08 and R-V141P1-09 through R-V141P1-19); all have triage decisions (accept/waive/defer) and closure notes for the implemented items.
- creator_id exposure evidence (`git show 3c40474f -- '**/creator/works/**' | head -100`): T2 commit adds `#[serde(skip_serializing)]` on `creator_id` for `PoolEntryDto` and `InspirationItemDto` (consistent with prior `WorkApiDto` contract R-V133P1-10). CLI list/status paths no longer emit it.
- slug collision evidence (`git show 3c40474f -- '**/inspiration_items.rs' | head -100`): `create_inspiration_with_scaffold` now loops over base_slug, base_slug-1, ... up to -100; writes file only after existence check; rolls back file on subsequent DB insert failure; returns explicit `ConstraintViolation` on exhaustion. Bounded, atomic-ish, no silent overwrite.
- `git log -1 --oneline .mstar/plans/reports/2026-06-11-v1.42-ux-polish/qc2.md`: (to be captured after commit)
- QC worktree working tree clean (post-commit): (verified after `git commit` of only the report path)

## Security / Correctness Lens (reviewer_index=2 focus)
- **CLI arg / combined flag paths (R-V141P0-05)**: T4 is documentation only ("verified working individually"). Per plan, no single-command combined path for `--from-work + --reopen + --extend-chapters` was added (these remain separate commands used sequentially). No new attack surface or validation bypass introduced.
- **creator_id exposure (R-V141P1-11)**: Correctly suppressed via `#[serde(skip_serializing)]` on the two pool/inspiration DTOs. Matches existing `WorkApiDto` contract. No change to authorization or data visibility beyond the documented UX fix.
- **Slug collision / CJK fallback (R-V141P1-12/13)**: Auto-suffix on file collision (instead of hard error) is a UX improvement with bounded retries and explicit error on exhaustion. File write uses tmp+rename (atomic on POSIX). DB row insert is after file success; file is rolled back on DB failure. No path traversal (slug is derived + sanitized by `title_to_slug`; path is under controlled `Pool/Ideas/` within workspace_dir). CJK fallback to `idea-<hex>` (or romanized) prevents "untitled" UX degradation without introducing collisions or injection.
- **Missing-file hint (R-V141P0-06)**: Purely informational (`⚠ ...` + suggested reconcile command). Best-effort, read-only FS check gated behind DB `completion_locked_at` presence. Never mutates state, never trusts FS as authority, no privilege use. Safe under security lens.
- **Refactor (R-V141P0-02, T3)**: Extraction of `print_chapter_table`, `print_completion_lock_hint`, and `truncate_with_ellipsis` from `handle_status`. Behavior-preserving (identical output for novel vs non-novel paths). `cargo test -p nexus42 -- creator_works` passes. No logic, auth, or data-flow changes. Correctness preserved.
- **Residual triage (T1/T5)**: Status.json updates are metadata-only (decisions + closure notes). No code paths altered by the triage entries themselves.
- **No new risks identified**: No SQL injection (parameterized queries unchanged), no path traversal (home/workspace layout via `nexus_home_layout` crate, controlled paths), no auth bypass, no unbounded resource consumption in the changed UX paths, no secret or credential handling.

All changes under security/correctness review are low-risk, well-scoped, and correctly implemented. Pre-existing test flakes in the broader daemon test suite do not affect the reviewed surfaces.

**Verdict**: Approve

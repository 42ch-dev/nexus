---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-15-v1.47-serial-completion-hardening"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p7
- Review Perspective: Performance and reliability risk (test runtime, observability/idempotency of completion paths)
- Report Timestamp: 2026-06-15T21:55:00Z

## Scope
- plan_id: 2026-06-15-v1.47-serial-completion-hardening
- Review range / Diff basis: merge-base: c549eec7215215dc4d67a724602db827f26f9927 + tip: HEAD
- Working branch (verified): feature/v1.47-serial-completion-hardening
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.47-p2-serial
- Files reviewed: 3
  - `.mstar/plans/2026-06-15-v1.47-serial-completion-hardening.md`
  - `crates/nexus-local-db/src/work_chapters.rs`
  - `crates/nexus-orchestration/src/capability/builtins/novel_chapter_transition.rs`
- Commit range: c549eec7215215dc4d67a724602db827f26f9927..06ba126f
- Tools run:
  - `cargo test -p nexus-local-db --lib -- spec_4_5_7`
  - `cargo test -p nexus-orchestration --lib -- spec_4_5_7`
  - `cargo test -p nexus42 --lib -- reject_produce_when_novel_complete`
  - `cargo test -p nexus-local-db --lib -- work_chapters`
  - `cargo test -p nexus-orchestration --lib -- novel_chapter_transition`
  - `cargo test -p nexus-local-db -p nexus42 -- next_chapter`
  - `cargo test -p nexus-orchestration --lib -- current_chapter`
  - `cargo test -p nexus-orchestration --lib -- completion`
  - `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus42 -- -D warnings`
  - `cargo +nightly fmt --all --check`

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S-001 — Add an explicit double-finalize idempotency test for `current_chapter`.**
  The new §4.5.7 #2 test verifies that `current_chapter` advances only on finalize, but it does not re-run finalize on the same chapter to assert the value stays stable. Adding such an assertion would harden the idempotency claim in the reviewer focus.
  - Location: `crates/nexus-orchestration/src/capability/builtins/novel_chapter_transition.rs`, test `spec_4_5_7_current_chapter_advances_only_on_finalize`.
  - Fix: After the first `draft → finalized` transition on ch1, invoke the same transition again (with `from_status: "finalized"` and `force: false`) and assert `current_chapter` remains `1`.

- **S-002 — Avoid leaking the test temp directory in `novel_chapter_transition::tests::fresh_pool`.**
  The helper uses `std::mem::forget(dir)` to keep the temp database file alive. While this follows the existing pattern in `crates/nexus-orchestration/tests/auto_chain.rs`, it leaks one temporary directory per invocation. For a single new test the impact is small, but as more tests adopt this helper the leaked directories accumulate.
  - Location: `crates/nexus-orchestration/src/capability/builtins/novel_chapter_transition.rs`, helper `fresh_pool` (lines 451–463).
  - Fix: Return `(SqlitePool, tempfile::TempDir)` and let the caller bind the directory, matching the helper in `crates/nexus-local-db/src/work_chapters.rs`.

## Source Trace
- Finding ID: S-001
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus-orchestration/src/capability/builtins/novel_chapter_transition.rs` lines 499–613
  - Confidence: High
- Finding ID: S-002
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus-orchestration/src/capability/builtins/novel_chapter_transition.rs` lines 451–463
  - Confidence: Medium

## Performance & Reliability Notes
- **Test runtime / hermeticity**: The three new tests are hermetic (fresh in-memory SQLite pool + migrations per test) and fast: the two `spec_4_5_7_*` tests in `nexus-local-db` finished in ~0.09s, the `nexus-orchestration` test in ~0.05s, and the full `work_chapters` module in ~1.76s. No external I/O or shared state is used.
- **Observability**: Production path #2 (`NovelChapterTransition`) already emits `tracing::info!`/`tracing::warn!` at key transitions (`chapter_transition_completed`, `current_chapter_advanced`, forced-finalize warning, frontmatter updates). Production paths #1 (`next_chapter`) and #3 (`is_work_completed`) do not have `tracing::info!`/`warn!` at their key transitions; this is pre-existing and not introduced by P2, but worth noting for future hardening.
- **Idempotency**: The implementation is idempotent — `advance_current_chapter` sets `works.current_chapter` to the just-finalized chapter number (it does not increment), so a repeated finalize of the same chapter cannot double-advance. There is no explicit test for this yet (see S-001).
- **Concurrent finalize attempts**: The transition path reads the current row status, validates the `from_status` guard, then updates. Under the project's local-first single-user invariant this is acceptable; a true concurrent-writer scenario would require an atomic claim helper. This behavior is pre-existing and outside P2 scope.
- **R-V138P1-01 closure evidence**: Verified that `reject_produce_when_novel_complete` (`nexus42` `run.rs:502`) returns a clear error when `target_stage == "produce" && next_chapter.is_none()`, is invoked before `build_schedule_for_stage` (`run.rs:716` before `run.rs:765`), and has three passing unit tests. AC3 (no spurious schedule when `next_chapter=None` and Work is complete) is satisfied.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

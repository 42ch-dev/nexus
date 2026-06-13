---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-13-v1.44-multi-volume-hardening"
verdict: "Approve"
generated_at: "2026-06-13"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p6
- Review Perspective: Performance and reliability risk (multi-volume completion query overhead, supervisor volume propagation cost, schedule payload carrying cost, regression test runtime budget, observability on incomplete-volume Works)
- Report Timestamp: 2026-06-13

## Scope
- plan_id: 2026-06-13-v1.44-multi-volume-hardening
- Review range / Diff basis: c54b1aa6..9c53d8f6
- Working branch (verified): iteration/v1.44
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 9
- Commit range: 22324ddc (T1/T2/T3 core fix) .. b7d27aa7 (style) + merge 9c53d8f6
- Tools run:
  - `git log --oneline c54b1aa6..9c53d8f6`
  - `git diff --stat c54b1aa6..9c53d8f6`
  - `cargo clippy --all -- -D warnings`
  - `cargo test -p nexus-orchestration --test supervisor_cross_volume`
  - `cargo test -p nexus-local-db`
  - `cargo test -p nexus-local-db --test v142_migration_fixes`
  - `cargo +nightly fmt --all --check`
  - Source reads: `work_chapters.rs::is_work_completed`, `supervisor.rs` NextChapter arm, `auto_chain.rs::{build,enqueue}_auto_chain_schedule`, `stage_gates.rs::{WorkFields,build_preset_input}`, `boot.rs::resume_auto_chain_work`, regression tests in `supervisor_cross_volume.rs`

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S-001 (reliability / test comment drift)**: The existing test-section comment at `crates/nexus-local-db/src/work_chapters.rs:1061` still reads "`is_work_completed`: true when all chapters finalized, **current_chapter >= total**, intake complete", but the V1.44 P2 implementation deliberately removed the flat `current_chapter >= total_planned_chapters` predicate. The new volume-aware logic uses `COUNT(*)` and `SUM(CASE WHEN status = 'finalized')` across all volumes instead. Update the section comment and the test docstring to avoid misleading future maintainers into re-adding the dropped check.
  - Source: `crates/nexus-local-db/src/work_chapters.rs:1061–1102`
  - Confidence: High

- **S-002 (reliability / sqlx static-query checking)**: The new completion-count query in `is_work_completed` uses runtime `sqlx::query()` with a `// SAFETY:` comment. This matches the pre-existing pattern in the same function, but `crates/nexus-local-db/AGENTS.md` directs static SQL to use compile-time checked `sqlx::query!()` / `sqlx::query_as!()`. Consider migrating both the `works` SELECT and the `work_chapters` aggregate query to compile-time macros in a future hygiene pass so that schema drift (e.g., a renamed `status` column or a future `volume` NULLability change) is caught at build time rather than runtime.
  - Source: `crates/nexus-local-db/src/work_chapters.rs:769–815`
  - Confidence: Medium

- **S-003 (reliability / single-volume preset-input shape change)**: The supervisor `NextChapter` arm now passes `Some(next_volume)` for every chapter loop, including single-volume Works where `next_volume == 1`. This means single-volume `novel-writing` schedules will now carry `"volume": 1` in `preset.input`. Existing templates that do not reference `volume` are unaffected, but any new or user-installed template that treats `volume` as "multi-volume only" may render unexpected "Volume 1" labels. Document this behavior in `novel-workflow-profile.md` or the preset template guidance so authors know `volume` is always present when the schedule originates from the supervisor auto-chain.
  - Source: `crates/nexus-orchestration/src/schedule/supervisor.rs:491–518`, `crates/nexus-orchestration/src/stage_gates.rs:215–221`
  - Confidence: Medium

## Source Trace
- Finding ID: S-001 / S-002 / S-003
- Source Type: git-diff + static analysis + test execution + crate AGENTS.md rule check
- Source Reference: commits 22324ddc, 233bc3f2, b7d27aa7, 9c53d8f6
- Confidence: High

## Performance / Reliability Assessment

| Concern (from Assignment) | Assessment | Evidence |
|---------------------------|------------|----------|
| `is_work_completed` query overhead vs V1.42 baseline | **Improved.** The old path ran one `works` SELECT, one `list_chapters()` (full row fetch + `Vec` construction), and a client-side `all()` scan. The new path runs one `works` SELECT and one aggregate `work_chapters` query that returns two integers. The aggregate can use the existing `idx_work_chapters_next_volume_aware(work_id, status, volume, chapter)` covering index. | `work_chapters.rs:802–822` vs pre-change `list_chapters()` + `chapters.iter().all(...)` |
| Supervisor `next_volume` propagation cost | **No extra DB calls.** `next_volume` is already computed by `evaluate_after_persist_volume_aware` → `next_chapter_volume_aware` (existing V1.42 query). The change only threads the already-fetched value through `enqueue_auto_chain_step` → `enqueue_auto_chain_schedule` → `WorkFields`. | `supervisor.rs:459`, `supervisor.rs:597–598` |
| `build_auto_chain_schedule` overhead | **Negligible.** Adds one `Option<i32>` field to `WorkFields` and a conditional JSON insert. No allocations beyond the existing schedule request. | `auto_chain.rs:246–262`, `stage_gates.rs:218–221` |
| Regression test runtime budget | **No regression.** `supervisor_cross_volume.rs` ran 8 tests in ~0.49s; `nexus-local-db` full suite ran 190 tests in ~10s; `v142_migration_fixes` ran 2 tests in ~0.08s. | Test output above |
| Error observability on incomplete-volume Works | **Unchanged.** `is_work_completed` still returns `false` without logging *which* volume/chapter is unfinished. The supervisor logs cross-volume transitions at `info` level, but there is no structured span for the aggregate completion check. Consider a low-cardinality tracing attribute if multi-volume Works become common. | `work_chapters.rs:767–823`, `supervisor.rs:496–505` |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

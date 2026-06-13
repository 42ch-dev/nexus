---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-13-v1.44-multi-volume-hardening"
verdict: "Approve"
generated_at: "2026-06-13"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (multi-volume completion predicate, supervisor volume propagation, cross-volume context preservation, race conditions)
- Report Timestamp: 2026-06-13

## Scope
- plan_id: 2026-06-13-v1.44-multi-volume-hardening
- Review range / Diff basis: c54b1aa6..9c53d8f6
- Working branch (verified): iteration/v1.44
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 9
- Commit range: 22324ddc..b7d27aa7 + merge 9c53d8f6
- Tools run:
  - `git log --oneline c54b1aa6..9c53d8f6`
  - `git diff --stat c54b1aa6..9c53d8f6`
  - `cargo clippy --all -- -D warnings`
  - `cargo test -p nexus-orchestration --test supervisor_cross_volume`
  - `cargo test -p nexus-local-db`
  - `cargo +nightly fmt --all -- --check`
  - `rg -n 'next_volume|is_work_completed|WorkFields|volume'` (per plan verification section)
  - Manual diff review of 22324ddc (T1/T2/T3), 233bc3f2 (T4), b7d27aa7 (style), 9c53d8f6 (merge)
  - Source reads: `work_chapters.rs::is_work_completed`, `supervisor.rs` NextChapter arm, `auto_chain.rs` (build/enqueue), `stage_gates.rs` (WorkFields + build_preset_input), new tests in `supervisor_cross_volume.rs`

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S-001 (observability)**: The new COUNT query in `is_work_completed` is correct and reduces round-trips vs the prior `list_chapters` + filter path, but adds no structured span or metric for "multi-volume completion check latency" or "volume row count skew". Consider adding a low-cardinality tracing attribute (e.g., `volume_count`) on the completion path in a follow-up if multi-volume Works become common. Not required for this plan.
- **S-002 (test hygiene)**: The new test names `f002_multi_volume_work_completed_all_volumes_finalized` and `f004_supervisor_enqueue_includes_volume_in_preset_input` are descriptive; consider a small doc comment on the test module or a `// AC1 / AC2` marker inside each to make the mapping to plan Acceptance Criteria even more explicit for future readers. Cosmetic only.

## Source Trace
- Finding ID: N/A (no findings above Warning)
- Source Type: git-diff + manual code review + test execution
- Source Reference: commits 22324ddc (core fix), 233bc3f2 (regression tests)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Evidence (per Assignment)
- `cargo clippy --all -- -D warnings`: clean (no warnings treated as errors).
- `cargo test -p nexus-orchestration --test supervisor_cross_volume`: 8 passed (existing 8 + 4 new F-004 / F-002 tests covering AC1/AC2 and negative cases).
- `cargo test -p nexus-local-db`: 190+ tests passed (including new multi-volume completion cases in work_chapters and v142_migration_fixes).
- `git log --oneline c54b1aa6..9c53d8f6`:
  ```
  9c53d8f6 merge(v1.44 P2): multi-volume completion + supervisor volume propagation
  b7d27aa7 style(P2): nightly fmt fixes after T4 regression tests
  233bc3f2 test(P2-T4): add multi-volume completion + volume propagation regression tests
  22324ddc fix(P2-T1/T2/T3): harden multi-volume completion + thread volume through supervisor chain
  ```
- `cargo +nightly fmt --all -- --check`: clean (no output).
- `rg` verification (plan §6) matches the expected symbols in the four core files.

## Correctness & Security Analysis (qc-specialist-2 focus)

**Predicate correctness (F-002 / R-V142P1-QC1-F-002)**:
- Old: `current_chapter >= total_planned_chapters && list_chapters().len() == total && all finalized`.
- Problem: chapter numbers reset per volume (vol1 ch3, vol2 ch1). Flat `current_chapter` + length check is semantically wrong for multi-volume Works.
- Fix: single runtime `SELECT COUNT(*) AS total_rows, SUM(CASE WHEN status='finalized'...) AS finalized_rows FROM work_chapters WHERE work_id = ?`.
- Then: `total_rows == expected && finalized_rows == expected`.
- This matches novel-workflow-profile.md §6.1 ("all rows finalized across volumes; row count == total_planned_chapters").
- Also removed the now-unnecessary `current_chapter` fetch for the novel completion path (still present in other progress surfaces).
- The query is parameterized; no injection surface.
- Single query reduces TOCTOU window vs prior two-roundtrip `list_chapters` path.

**Supervisor volume propagation (F-004 / R-V142P1-QC1-F-004)**:
- `ChainAction::NextChapter { next_volume, next_chapter }` arm now correctly passes `Some(next_volume)` as the 5th arg to `enqueue_auto_chain_step`.
- Signature extension: `enqueue_auto_chain_step(..., chapter: Option<i32>, volume: Option<i32>, ...)`.
- `enqueue_auto_chain_schedule` and `build_auto_chain_schedule` now take and forward `volume: Option<i32>`.
- `WorkFields` gained `pub volume: Option<i32>`.
- `build_preset_input` injects `"volume": N` into the map when present (only for `novel-writing` preset input today).
- Negative case covered: single-volume enqueue explicitly passes `None` and test asserts `input.get("volume").is_none()`.
- Cross-volume context is now available to the preset as `{{preset.input.volume}}` (e.g., "Volume 2, Chapter 1").

**Race conditions between concurrent enqueue + finalize**:
- Supervisor actor serializes schedule processing per Work (existing design).
- Completion predicate is now a pure aggregate read; no mutation.
- No new shared mutable state or locking primitives introduced in this diff.
- The prior `list_chapters` + client-side filter had a larger read window; the COUNT is atomic at the query level.
- No evidence of concurrent finalization + enqueue races being introduced or worsened.

**Other security / correctness surfaces**:
- All new SQL is runtime `sqlx::query` with `// SAFETY:` comments (per crate policy for aggregates / non-static shapes); no compile-time macro available for the conditional SUM.
- No filesystem paths, no user-controlled strings in queries, no privilege escalation paths.
- Volume is carried as `Option<i32>` with explicit `Some`/`None` at call sites — no silent default to 1 that could mask a propagation bug.
- Tests are hermetic (fresh `test_pool`), cover both positive ACs and the explicit negative cases required by the plan.

**Residual scope correction**:
- Code comments and new test names now correctly attribute the completion predicate to `work_chapters::is_work_completed` (not `auto_chain.rs`).
- This aligns with the compass and prior QC note that the predicate lived in the wrong file.

## Alignment with Inputs
- Plan ACs 1–5 are met by the added tests and the predicate/propagation changes.
- Matches compass §1.6 (P2 required residuals) and §4 (Acceptance for closing).
- No P0/P1 changes in scope.
- Diff basis and review range match Assignment verbatim.

**Verdict**: Approve (0 Critical, 0 Warning). The changes are a focused, well-tested correctness hardening with no new security surface and reduced race window on the completion check.

---
report_kind: qc_review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-18-v1.51-cross-chapter-rescan
verdict: Approve
generated_at: 2026-06-18T17:14:55Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-18T17:14:55Z

## Scope
- plan_id: 2026-06-18-v1.51-cross-chapter-rescan
- Review range / Diff basis: iteration/v1.51...HEAD (= 00829432...3d7c1f23)
- Working branch (verified): feature/v1.51-cross-chapter-rescan
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-a-p1
- Files reviewed: 9 (diff stats: +1751/-22)
- Commit range: 008294327a8a33714948eb6d810794d338ceaa93..3d7c1f23c2de31466d3c87ae2e789f08981d1fe2
- Tools run:
  - `git diff iteration/v1.51...HEAD` + `git log iteration/v1.51..HEAD` (5 commits, per-task granularity)
  - `cargo test -p nexus42 --test kb_rescan` (11 passed, 0 failed)
  - `cargo test -p nexus-orchestration --lib quality_loop::tests::aggregate_` (8 passed, 0 failed)
  - `cargo test -p nexus42 --test kb_rescan_cli` (8 V1.50 chapter-scoped regression tests passed, unchanged)
  - `cargo test -p nexus-local-db --test kb_extract_jobs_upsert` (6 passed, 0 failed)
  - `cargo test -p nexus-local-db --test kb_extract_jobs_migration` (12 passed, 0 failed — 8 V1.50 + 4 V1.51)
  - `cargo test -p nexus42 --test cli_lock_contention` (3 T-B P0 lock passed)
  - `cargo test -p nexus-local-db --test file_lock` (3 T-B P0 lock passed)
  - `cargo test -p nexus42 --test creator_world_kb_adopt` (3 T-A P0 adopt regression passed)
  - `cargo test -p nexus-kb --lib extract_sync` (7 V1.50 extract_sync regression passed)
  - `cargo test -p nexus-orchestration --lib llm_extract` (15 T-A P0 LLM pathway regression passed)
  - `cargo test -p nexus-orchestration --test novel_review_master` (3 T-A P0 novel-review-master regression passed)
  - `cargo +nightly fmt --all --check` (exit 0, clean)
  - `cargo clippy --all -- -D warnings` (exit 0, **clean** on this branch HEAD)
  - Source trace: `aggregate_candidates_by_canonical_name` (quality_loop.rs:309-376), `extract_per_chapter` (rescan.rs:466-494), `acquire_work_lock` (rescan.rs:498-510), `sync_work_candidates` (rescan.rs:519-600), `delete_pending_for_chapter_work` (rescan.rs:634-650), `preview_work_candidate_outcome` (rescan.rs:603-624), CLI dispatch (kb.rs:245-261), file_lock heartbeat lifecycle (file_lock.rs:191-266), `list_pending_for_world` cap (kb_extract_job.rs:872-893)

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion

- **S-01 (stale residual `R-V151-MERGE-CLIPPY-01`)**: The completion report's verification step 11 records that `cargo clippy --all -- -D warnings` failed on `iteration/v1.51` HEAD `388602d2` with one pre-existing error (`kb_adopt too_many_lines 118/100`), and registered the residual `R-V151-MERGE-CLIPPY-01` as `lifecycle: "open"` with `target: "V1.51 P-last WL-A"`. The reviewer verifies the pre-existing-claim was correct **for `388602d2`** but the **current diff basis `00829432`** already includes commit `00829432 fix(nexus42): surgical hygiene — #[allow(clippy::too_many_lines)] on kb_adopt` which addresses that exact lint. The full T-A P1 commit stack compiles clippy-clean on top of `00829432`: `cargo clippy --all -- -D warnings` exits 0 on the current branch HEAD. The residual is therefore stale at the time of this QC review (pre-existing claim was valid when written, but the underlying problem is already in the diff basis). Per QC NEVER rules, this reviewer does not modify residual lifecycle; PM/QA should mark `R-V151-MERGE-CLIPPY-01` as `resolved` (or downgrade to `waived`) with closure note citing commit `00829432`. Not a blocker for this plan — the implementation is clippy-clean.

- **S-02 (`list_pending_for_world` hard cap at 500)**: The work-scoped rescan loads the stale-cleanup baseline via `nexus_local_db::kb_extract_job::list_pending_for_world(pool, &world_id, None)` (rescan.rs:404-407). That function (kb_extract_job.rs:872-893) hard-caps the result at 500 rows (`clamp(1, 500)`, default 100), so for a world with more than 500 pending candidates, stale candidates beyond the cap are not seen by the cleanup loop and will not be removed by `delete_pending_for_chapter_work`. The chapter-scoped path uses `list_for_chapter` (no cap) and is unaffected. At V1.51 scale (small works) this is unreachable in practice, but is a soft scalability concern for very large worlds. Two possible mitigations: pass an explicit `Some(500)` to make the cap visible at the call site, or split the cleanup loop into a per-page iteration. No residual opened (acceptable for V1.51; future P-last WL-A can address if the cap proves reachable in production telemetry).

- **S-03 (lock skipped if work directory missing)**: The lock acquisition in `kb_rescan_work_hermetic` (rescan.rs:426-431) has a defensive `if dry_run || !work_dir.exists() { None } else { Some(acquire_work_lock(&work_dir)?) }` branch. If a work has chapters listed in `work_chapters` with valid `body_path`s but the `Works/<work_ref>/` directory on disk has been deleted (e.g., partial filesystem state), the non-dry path proceeds **without** the advisory lock. This is an edge case unlikely under normal CLI usage (chapters are read from `body_path`, which is workspace-relative, not work-dir-relative), but the spec wording at `world-kb-runtime-architecture.md §5.5.1` is "the non-dry work-scoped path acquires `Works/<work_ref>/.lock` before the cross-chapter upsert" without an existence exception. The defensive behavior is acceptable for a sync tool, but a one-line code comment in `kb_rescan_work_hermetic` (or a `tracing::warn!` when the skip fires) would make the trade-off visible to operators. No residual opened (low priority; documentation only).

## Performance & Reliability Trace (per-assignment focus)

### Hot-path overhead (per-chapter cost)
- `extract_per_chapter` (rescan.rs:466-494): per chapter = 1 × `list_chapters` row materialization + 1 × `std::fs::read_to_string` + 1 × `extract_candidates_from_text` (heuristic regex `capitalized_phrase_regex` + dedup, bounded by `MAX_CANDIDATES_PER_PASS = 20`). At N chapters, total cost is **O(N × prose_size)** for I/O + **O(N × 20)** for extraction. Sequential single-threaded I/O (no parallelization); acceptable for a CLI sync tool at V1.51 scale.
- `aggregate_candidates_by_canonical_name` (quality_loop.rs:309-376): HashMap keyed by lowercased canonical_name, parallel accumulators, BTreeSet for chapter dedup. Total cost is **O(total_candidates)** (≈ O(N × 20) for the bounded extractor) — assignment's "O(N) with hash map" expectation **met** within the `MAX_CANDIDATES_PER_PASS` bound.
- Verified by `aggregate_*` unit tests (8/8): collapses same name across chapters, keeps distinct names separate, case-insensitive + first-seen case preserved, dedupes chapter list, empty input → empty output, skips empty-chapter entries, preserves LLM metadata when present, records `source_chapters` in payload JSON.

### DB write amplification
- `sync_work_candidates` (rescan.rs:519-600): one `upsert_pending_candidate` call **per aggregate**, NOT per chapter. The DB unique constraint `(creator_id, work_entry_id=canonical_name_guess, world_id) WHERE status NOT IN ('failed')` (V1.50 P1 migration) collapses N same-name per-chapter candidates to 1 row. Verified by `cross_chapter_same_entity_collapses_to_one_pending_row` test (3 chapters with "Aelin" → 1 pending row, `source_chapters: [1, 2, 3]`).
- Stale cleanup: one `DELETE FROM kb_extract_jobs WHERE work_id = ? AND canonical_name_guess = ? AND promotion_status = 'pending'` per stale name (rescan.rs:634-650). Bounded by stale candidate count, not chapter count.
- KB refresh: `diff_and_apply` is O(K) for K existing KeyBlocks (chapter-scoped equivalent) — same complexity as V1.50 path.

### LLM call cost
- **No LLM call** in the work-scoped rescan path. `kb_rescan_work_hermetic` calls `extract_candidates_from_text` (heuristic only) per chapter. The `nexus.llm.extract` LLM pathway (T-A P0) is preserved (15/15 `llm_extract` tests + 3/3 `novel_review_master` tests pass) and is unaffected. The spec design decision is documented at `world-kb-runtime-architecture.md §5.5.1`: "Work-scoped rescan uses the **heuristic** ... Wiring the rescan to the `nexus.llm.extract` LLM pathway ... is **out of scope** for T-A P1 (the LLM pathway is a review-time/finalize-time concern; rescan is a sync tool)."
- `aggregate_candidates_by_canonical_name` is **pathway-agnostic** (operates on `KbCandidate` regardless of source) — a future plan can swap the extractor without touching the reconciliation logic. Forward-looking, no immediate cost.

### Lock contention handling
- `acquire_work_lock` (rescan.rs:498-510) maps `FileLockError::Locked` → `CliError::Locked { holder_pid, holder_name, stale }` (exit 75, `EX_TEMPFAIL`) and `FileLockError::Io` → `CliError::LockIo` (exit 78, `EX_CONFIG`). The dual exit-code contract from T-B P0 is preserved. Verified by `cross_chapter_lock_contention_returns_e_lock` and `cross_chapter_lock_io_failure_returns_e_lock_io` tests.
- Lock is acquired **after** chapter reads (rescan.rs:372-431) but **before** the DB upsert and KB refresh — the spec invariant "file-lock-before-DB, never reversed" is satisfied. Heavy I/O (chapter body reads + heuristic extraction + aggregation) happens before the lock, minimizing lock-hold time.
- Daemon cron-side fire of `novel-brainstorm` while author runs `creator kb rescan --work` non-dry: cron-side uses the same `Works/<work_ref>/.lock`; either side will receive `E_LOCK` (exit 75) on contention. Per T-B P0 §3 behavior, cron retries on next tick. **Behavior matches the compass §1.1 acceptance #4** expectation.

### `--dry-run` cost
- `cross_chapter_dry_run_succeeds_under_lock_contention` test (kb_rescan.rs:378-392) confirms dry-run is **read-only** and acquires **no** advisory lock (lock is held externally throughout the dry-run call). The dry path is identical to non-dry up to the DB upsert point, then takes a `preview_work_candidate_outcome` branch (rescan.rs:603-624) instead of `upsert_pending_candidate`. Cost is dominated by the chapter scan + aggregation; DB writes are skipped. **`--dry-run` cost is comparable to non-dry minus the SQL writes + lock syscalls** — no I/O amplification.
- The `cross_chapter_dry_run_shows_reuse_summary_without_writing` test verifies the dry-run `cross_chapter_reuse` summary is populated correctly (chapters + existing KB row flag) and no rows are written.

### Idempotency under interruption
- `upsert_pending_candidate` (V1.50 T-B P2) is idempotent on the unique constraint `(creator_id, work_entry_id, world_id) WHERE status NOT IN ('failed')`. Partial failure mid-rescan (e.g., process killed after some upserts but before stale cleanup) leaves a consistent state: re-running the same rescan converges to the same final state because the next call's upsert is a no-op for already-up-to-date rows and the stale cleanup is a no-op for already-removed rows.
- Verified by `cross_chapter_idempotent_rerun_produces_empty_candidate_diff` test: second rescan on unchanged text → `candidates_inserted.is_empty() && candidates_updated.is_empty() && candidates_unchanged == 1`.
- The `TODO(T-B P1)` marker in `sync_work_candidates` (rescan.rs:537-542) is the planned future swap to versioned CAS; the advisory lock + DB unique constraint together provide cross-process + per-row guards today.

### Error observability
- `tracing::warn!` / `tracing::info!` / `tracing::error!` at appropriate levels in the rescan path and in `file_lock` heartbeat. Lock failures are reported with holder PID + holder name + stale flag (`cross_chapter_lock_contention_returns_e_lock` test asserts `holder_name == "test:holder"`). DB errors include the table/operation context. Chapter read failures include the body path. CLI error messages are actionable: "Work '{work_ref}' not found for creator '{creator_id}'. Usage: creator kb rescan --work <work_ref>" / "Work '{work_ref}' has no world_id; cannot rescan KB. Bind the work to a world first." / cross-author 403 surfaces `WORLD_KB_FORBIDDEN` stable code.
- Dry-run prints the cross-chapter reuse summary (`Entity 'Aelin' referenced in chapters 1, 2, 3; existing KB row found → no new candidate`) — operators can see the cross-chapter provenance before committing.

### Regression checks
- V1.50 chapter-scoped rescan (`kb_rescan_cli`): **8/8 unchanged** — `kb_rescan_hermetic` and `kb_rescan` paths are untouched (additive diff only; no removed functions, no signature changes for V1.50 callers). Verified by reading the function-diff (`-` lines are 0 for V1.50 entrypoints).
- T-A P0 LLM extraction (`llm_extract`, `novel_review_master`): **15/15 + 3/3 passed** — `aggregate_candidates_by_canonical_name` is additive; the existing review-time hook + LLM path are untouched.
- T-B P0 advisory lock (`file_lock`, `cli_lock_contention`): **3/3 + 3/3 passed** — `acquire_work_lock` reuses the existing `file_lock::try_acquire` API; no changes to the lock primitive.
- T-A P0 adopt (`creator_world_kb_adopt`): **3/3 passed** — adopt path untouched.
- V1.50 extract_sync (`nexus-kb --lib extract_sync`): **7/7 passed** — KB diff/apply unchanged.
- V1.50 DB upsert (`kb_extract_jobs_upsert`): **6/6 passed** — the existing `upsert_pending_candidate` is reused; no schema change.
- V1.50 DB migration (`kb_extract_jobs_migration`): **12/12 passed** (8 V1.50 + 4 V1.51) — no migration added by T-A P1; the additive `source_chapters` JSON key is a payload-level extension.

## Source Trace

- **F-PERF-01 — work-scoped hot path**: `kb_rescan_work_hermetic` (rescan.rs:338-459) → `extract_per_chapter` (rescan.rs:466-494) → `aggregate_candidates_by_canonical_name` (quality_loop.rs:309-376) → `acquire_work_lock` (rescan.rs:498-510) → `sync_work_candidates` (rescan.rs:519-600) → `sync_work_kb_rows` (rescan.rs:658-693). Confidence: High.
- **F-PERF-02 — DB write amplification**: `upsert_pending_candidate` called once per aggregate (rescan.rs:543-559), keyed on the V1.50 P1 unique constraint. Verified by `cross_chapter_same_entity_collapses_to_one_pending_row` test asserting 3 chapters → 1 row. Confidence: High.
- **F-PERF-03 — no LLM re-call**: `kb_rescan_work_hermetic` calls only `extract_candidates_from_text` (rescan.rs:375). The LLM pathway is preserved but not invoked. Spec quote: `world-kb-runtime-architecture.md §5.5.1` "Extraction pathway used. Work-scoped rescan uses the **heuristic** ... is out of scope for T-A P1". Confidence: High.
- **F-PERF-04 — dry-run is read-only**: `if dry_run || !work_dir.exists() { None } else { Some(acquire_work_lock(&work_dir)?) }` (rescan.rs:427-431) + `preview_work_candidate_outcome` (rescan.rs:603-624) + `compute_kb_diff` for KB (rescan.rs:678-679). Verified by `cross_chapter_dry_run_succeeds_under_lock_contention` and `cross_chapter_dry_run_shows_reuse_summary_without_writing` tests. Confidence: High.
- **F-PERF-05 — idempotency**: DB unique constraint + `upsert_pending_candidate` semantics. Verified by `cross_chapter_idempotent_rerun_produces_empty_candidate_diff` test. Confidence: High.
- **F-PERF-06 — lock acquisition cost**: `acquire_work_lock` is one `try_acquire` syscall before the DB upsert; cost is microseconds. The heartbeat task is `tokio::spawn`-ed with a watch channel cancellation; `FileLockGuard::Drop` releases the `flock` and aborts the heartbeat (file_lock.rs:268-292). Confidence: High.
- **F-PERF-07 — stale residual R-V151-MERGE-CLIPPY-01**: The pre-existing claim (clippy regression at `388602d2`) is correct historically. The current diff basis `00829432` is **after** the `00829432 fix(nexus42): surgical hygiene — #[allow(clippy::too_many_lines)] on kb_adopt` commit, so the underlying problem is already addressed. `cargo clippy --all -- -D warnings` on the current branch HEAD exits 0. Confidence: High.
- **F-PERF-08 — `list_pending_for_world` cap**: kb_extract_job.rs:872-893 hard-codes `clamp(1, 500)`, default 100. Stale cleanup loop (rescan.rs:573-598) iterates only the loaded rows. Confidence: High.
- **F-PERF-09 — work-dir existence check**: rescan.rs:426-431 has a defensive `!work_dir.exists()` exception. Confidence: High.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

The V1.51 T-A P1 cross-chapter rescan implementation is performance- and reliability-clean:

- **Hot-path overhead**: O(N × prose_size) for I/O + O(N × 20) for heuristic + aggregation (HashMap, bounded by `MAX_CANDIDATES_PER_PASS`). Within the assignment's "O(N) with hash map" expectation.
- **DB write amplification**: 1 `upsert_pending_candidate` per aggregate (NOT per chapter), confirmed by `cross_chapter_same_entity_collapses_to_one_pending_row` test (3 chapters → 1 row, `source_chapters: [1, 2, 3]`).
- **No LLM re-call**: heuristic-only path; LLM pathway preserved (15/15 `llm_extract` + 3/3 `novel_review_master`).
- **Lock contention handling**: dual exit-code contract preserved (Locked → 75, LockIo → 78); dry-run read-only; daemon cron-side retry-on-conflict per T-B P0.
- **`--dry-run` cost**: comparable to non-dry minus DB writes + lock; no I/O amplification.
- **Idempotency**: DB unique constraint + `upsert_pending_candidate` semantics; re-run converges to same state.
- **Error observability**: tracing at appropriate levels; CLI error messages actionable.
- **Regression**: V1.50 chapter-scoped, T-A P0 LLM, T-B P0 lock, T-A P0 adopt, V1.50 extract_sync/upsert/migration — all preserved (clippy clean, nightly fmt clean, all test suites green).
- **CI gates**: `cargo +nightly fmt --all --check` exit 0; `cargo clippy --all -- -D warnings` exit 0 on this branch HEAD (the `R-V151-MERGE-CLIPPY-01` residual is now stale — see S-01).

The three Suggestion findings are documentation / cleanup items for the PM/QA residual lifecycle, not blockers. Approve.

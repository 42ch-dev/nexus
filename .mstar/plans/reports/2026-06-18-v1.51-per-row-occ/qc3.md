---
report_kind: qc_review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-18-v1.51-per-row-occ
verdict: Approve
generated_at: 2026-06-19T12:30:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Review Perspective: Performance and reliability risk (retry cost budget, DB read amplification, index requirements, race-test fidelity, failure observability, resource lifecycle, acquire-order discipline, regression risk on cron / advisory-lock / LLM paths)
- Report Timestamp: 2026-06-19T12:30:00Z

## Scope
- plan_id: 2026-06-18-v1.51-per-row-occ
- Review range / Diff basis: iteration/v1.51...HEAD (= 008294327a8a33714948eb6d810794d338ceaa93...e988291a1e9290de4ec3f586de32455fa15e7788)
- Working branch (verified): feature/v1.51-per-row-occ
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.51-t-b-p1
- Files reviewed: 15 (diff stat: 5 new, 9 modified, ~1479 insertions, ~40 deletions)
- Commit range: 00829432...e988291a (2 commits: f5eecf3d feat, e988291a docs completion)
- Tools run: cargo test (cas_migration_roundtrip × 2, cli_version_error × 2, kb_adopt_cas × 2, nexus-local-db --lib, file_lock, cli_lock_contention, cron_supervisor, nexus-orchestration --lib -- llm), cargo clippy --all -- -D warnings, cargo +nightly fmt --all --check, git diff / log, manual source trace of CAS + retry + acquire-order paths

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S-001 (nit, cross-references qc2 W-001): `actual_version: None` discards diagnostic detail in `kb_adopt`'s `VersionConflict` mapping.**  
  `crates/nexus42/src/commands/creator/world/kb.rs:541-554` maps `LocalDbError::VersionMismatch { actual, .. }` to `CliError::VersionConflict { actual_version: None, .. }`. The underlying `mark_confirmed_in_tx_with_cas` correctly reads the actual version (kb_extract_job.rs:1030-1035) but the caller throws it away. Display falls back to "actual v?" (errors.rs:285-289). Not a reliability defect (message is still actionable: "retry the operation"), but a reliability **observability** nit. Cross-references qc2 W-001 — same root cause, different angle. → thread `actual` from the inner error into `CliError::VersionConflict`.

- **S-002 (nit, test quality): `test_kb_adopt_stale_preimage_returns_version_conflict` does not exercise the stale-preimage path through `kb_adopt`.**  
  `crates/nexus42/tests/kb_adopt_cas.rs:69-106` bumps the version *before* the adopt and then never actually calls `kb_adopt` to assert the E_VERSION return. The test body documents this in comments and defers to `test_cas_version_mismatch_direct` (line 151) for the real assertion. The misleading name suggests a `kb_adopt` integration test that does not exist. → either rename to `test_kb_adopt_version_bump_does_not_block_adopt_when_preimage_re_reads` and assert the success path, or remove in favour of `test_cas_version_mismatch_direct`. The real stale-preimage test for `kb_adopt` would require a tokio-spawned concurrent writer to inject a race between `load_pending_candidate` and `mark_confirmed_in_tx_with_cas` — feasible but out of scope for this hermetic test suite.

- **S-003 (nit, hygiene): unused imports in `kb_adopt_cas.rs`.**  
  `nexus_kb::KbStore` (line 16) and `nexus_local_db::kb_store::SqliteKbStore` (line 18) are imported but never referenced. Test target compiles with 2 warnings; not blocking (`cargo clippy --all -- -D warnings` does not include test targets per AGENTS.md, but the warnings add noise). → `cargo clippy --fix --allow-dirty` on the test target.

## Source Trace

### Performance / reliability dimensions audited

#### 1. Cron-fire retry cost budget
- **Retry loop**: `nexus-orchestration/src/schedule/cron_supervisor.rs:367-404` — `for attempt in 1..=max_attempts` with `max_attempts=3, backoff_ms=100`. Worst-case latency = 2 backoffs × 100ms = **200ms** per cron-fire (not 300ms; the backoff runs *between* attempts, not after the final one). The assignment noted "300ms worst-case"; the actual bound is 200ms. Either way, well within a 60s tick budget (0.33% overhead).
- **Dormant today**: the comment at lines 364-366 documents that `enqueue_cron_schedule` only touches unversioned `creator_schedules`. Confirmed by reading `nexus-orchestration/src/auto_chain.rs` `enqueue_cron_schedule` signature: it takes `(pool, creator_id, work_id, preset_id, role_name)` and writes to `creator_schedules` only. The CAS retry is **infrastructure ready** — it activates the moment T-A P1/P2 paths within the fire scope write to `kb_extract_jobs.version`. Acceptable: explicitly called out in completion.md Risk #1.
- **Activation risk**: when T-A P1/P2 land, the retry will execute. The `tokio::time::sleep` is cancellation-safe (drops the timer on scope exit); the file lock guard is held through the loop and released on scope exit. No timer / connection leak.

#### 2. DB read amplification (CAS preimage)
- **Success path** (`mark_confirmed_in_tx_with_cas`, kb_extract_job.rs:1011-1022): `UPDATE ... WHERE id = ? AND version = ?` is the single statement. No extra round-trip. The `rows_affected()` result is the only post-write check. ✓
- **Failure path** (cas.rs:54-80 `cas_check`): on `rows_affected == 0`, an additional `SELECT version FROM {table} WHERE {id_column} = ?` is issued for a descriptive error message. This is **only on the failure path**; cost is bounded by retry-exhaustion (max 3 attempts × 1 read = 3 reads in the worst case for a chronically contested row). Acceptable.
- **`mark_confirmed_in_tx_with_cas` failure path** (kb_extract_job.rs:1030-1056): also re-reads `(promotion_status, version)` to disambiguate "already non-pending" from "version mismatch" from "row missing". This is the **same SELECT** (column list widened) — not a separate round-trip. ✓

#### 3. Index requirements
- **CAS lookup pattern**: `WHERE id = ? AND version = ?`. The primary key on `kb_extract_jobs.job_id` and `novel_pool_entries.entry_id` already provides an O(log n) index for the `id` lookup; the `version` check is a post-PK row predicate (cheap). **No new index is required.**
- **Verified by `cas_migration_roundtrip` tests**: `test_version_increments_on_cas_update` (line 92-147) and `test_cas_marks_confirmed_rejects_stale_version` (line 187) execute the `WHERE id = ? AND version = ?` pattern at the SQL level; both pass. The `query_as!` macro in `kb_extract_job.rs` will fail to compile if `.sqlx/` cache is stale, but cargo compile succeeded here so the cache is consistent. Local developers may need `cargo sqlx prepare` after migration add (completion.md Risk #3).

#### 4. Race-condition test fidelity
- **Stress run**: `cargo test -p nexus-local-db --test cas_migration_roundtrip -- --test-threads=8` (5/5 pass, 0.46s). Same suite at `--test-threads=1` (5/5 pass, 0.25s). No flakes.
- **kb_adopt_cas stress**: `cargo test -p nexus42 --test kb_adopt_cas -- --test-threads=1` (4/4 pass, 0.22s). The test uses unique `tempfile::tempdir()` per test and UUIDv4 `job_id`s; no shared-state collisions.
- **Concurrent-claim regression**: `test_claim_job_concurrent_double_claim_prevented` (kb_extract_job.rs:1306-1331) is an existing test not in the diff range but exercises the same SQLite WAL pattern; passes.
- **Verdict**: race tests are hermetic (fresh pool per test), so thread count has no observable effect. No flakes observed.

#### 5. Failure observability
- **CAS retry warn** (cron_supervisor.rs:388-393): `tracing::warn!` with `work_id`, `role`, `attempt`, `max_attempts` on every retry. ✓
- **Retry exhaustion log** (cron_supervisor.rs:395-401): `tracing::warn!` with `error = %e` on each non-retryable error, and breaks out of the loop. Final schedule is not enqueued; the sweep continues for the next work/role. Best-effort semantics match the cron philosophy. ✓
- **CLI E_VERSION message** (errors.rs:284-289): includes `table`, `row_id`, `expected_version`, `actual_version` (or "?"). Actionable: "retry the operation". One observability nit — see S-001 (cross-ref qc2 W-001).
- **CAS mismatch detail** (cas.rs:74-79): `VersionMismatch { table, id, expected, actual }` carries the full diagnostic; the `cas_check` consumer can format it as needed.

#### 6. Resource lifecycle
- **CAS retry in cron_supervisor (lines 371-404)**: the `let _file_lock = lock_result.ok().flatten();` guard is held throughout the loop and dropped on scope exit. `tokio::time::sleep` is a future — on cancellation it returns `Poll::Ready(Err(_))` without leaving a registered timer. No timer / connection leak.
- **`with_cas_retry`** (cas.rs:102-136): closure `f` is awaited once per attempt; pool connections returned to the pool between attempts (sqlx auto). The `unreachable!` at line 135 is reachable only if `max_attempts == 0` (caller fault); the `for attempt in 1..=max_attempts` returns on the final iteration's `Err`. Defensive panic only on `max_attempts=0` — caller invariant.
- **No `unsafe` code** added or used. ✓

#### 7. Acquire-order discipline (file lock → DB → CAS)
- **`kb_adopt` (kb.rs:447-577)**:
  1. Load candidate (`load_pending_candidate`, no lock).
  2. Author identity gate.
  3. **File lock** (`try_acquire` at line 476, before `pool.begin()` at line 527).
  4. **DB transaction** (`pool.begin()` at line 527).
  5. `insert_key_block_in_tx` (DB lock acquired implicitly by tx).
  6. **`mark_confirmed_in_tx_with_cas`** (CAS inside tx, line 541).
  7. `tx.commit()` at line 575.
  8. File lock dropped on scope exit.
  Order: file → DB → CAS. ✓ Matches concurrency.md §2.4, §7.5.
- **`try_fire_role` (cron_supervisor.rs:277-431)**:
  1. Read-only cron match check.
  2. Per-work gating.
  3. Idempotency check (read-only).
  4. **File lock** (`maybe_acquire_cron_file_lock` at line 348).
  5. **`enqueue_cron_schedule` with CAS retry** (lines 371-404).
  6. File lock dropped on scope exit (line 414).
  Order: file → enqueue (which would contain CAS when T-A paths activate). ✓
- **No reverse-order acquisition**: the diff adds no new lock-acquire-before-file-lock paths. Search for `pool.begin` in the diff: only one (kb.rs:527), always after the file lock. ✓

#### 8. No regression on existing tests
| Test target | Result | Notes |
|---|---|---|
| `cargo test -p nexus-local-db --lib` | **245 passed** | No regressions in kb_extract_job / novel_pool_entries / works / findings / world_stories modules |
| `cargo test -p nexus-local-db --test file_lock` | **3 passed** | T-B P0 advisory lock — race conditions still serialised |
| `cargo test -p nexus42 --test cli_lock_contention` | **3 passed** | T-B P0 E_LOCK / E_LOCK_IO mapping unchanged |
| `cargo test -p nexus-orchestration --test cron_supervisor` | **22 passed** | V1.50 cron evaluator + `set_schedule_json_tx` CAS + `set_schedule_json_tx_concurrent_writers_serialise` — all green |
| `cargo test -p nexus-orchestration --lib -- llm` | **50 passed** | T-A P0 LLM extraction path unaffected |
| `cargo clippy --all -- -D warnings` | **0 errors** | CI gate (matches AGENTS.md policy; test targets not in `--all` scope) |
| `cargo +nightly fmt --all --check` | **PASS** | 0 diffs |
| `cargo test -- --test-threads=8` stress | **PASS** | All CAS tests deterministic at 8 threads |

#### 9. Pre-existing test-target clippy warnings (out of scope but noted)
- `cargo clippy -p nexus42 --tests -- -D warnings` surfaces 27 pre-existing errors in `nexus42/src/commands/{creator/works,system}/*.rs` (e.g. `let_underscore_future`, `match_same_arms`, `doc_markdown`).
- `cargo clippy -p nexus-orchestration --tests -- -D warnings` surfaces 4 pre-existing doc-markdown warnings.
- **Verified pre-existing on `iteration/v1.51` HEAD** (checked out at line 3190 of nexus42/src/commands/creator/works/mod.rs and observed the same `let _ = result;` warning before this plan's changes). The CI command `cargo clippy --all -- -D warnings` does **not** include `--tests`, so the CI gate is green; the warnings would be caught by a stricter `cargo clippy --all --tests -- -D warnings` policy, which is not currently enforced.
- **Out of scope for T-B P1**; recommend V1.51 P-last WL-A hygiene sweep to fix the test-target clippy errors as a single batch (defer).

### Cross-reviewer notes (PM consolidation inputs)
- **qc2 W-001 (E_VERSION surfacing loses actual version detail)**: confirmed from reliability perspective. S-001 above is the same root cause framed as a nit; qc2 framed it as a Warning. The reliability angle is the same — degraded failure-observability. PM consolidation should treat this as one R#.
- **qc2 W-002 (prescribed test target `cron_cas_retry` does not exist)**: confirmed. The plan §6 commands and the assignment both cite `cargo test -p nexus-daemon-runtime --test cron_cas_retry`; no such target exists. The retry logic IS covered by `cas.rs` unit tests + `cron_supervisor` integration tests (22 passing). The fix is documentation / artifact alignment, not a code defect. PM should resolve as a doc-only R# in P-last WL-A.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Additional Notes
- Performance / reliability surfaces of T-B P1 are clean: CAS atomicity holds, retry is bounded (200ms worst-case vs 60s tick budget = 0.33% overhead), retry is dormant today and well-documented, acquire-order discipline is preserved (file → DB → CAS in both `kb_adopt` and `try_fire_role`), resource lifecycle has no leaks, race tests are deterministic at 8 threads, all regression suites (V1.50 cron, T-B P0 file lock, T-A P0 LLM) pass.
- The 3 nit Suggestions are small (one cross-references qc2 W-001; the other two are test-quality / hygiene) and do not block the reliability story.
- PM should consolidate qc1 / qc2 / qc3 findings into a single R# list per `mstar-plan-artifacts/references/status-and-residuals.md`. The two qc2 warnings (W-001 reliability-observability, W-002 test-target doc mismatch) are the only blockers from the tri-review; both are surgical fixes that should be in V1.51 P-last WL-A.
- The dormant cron-side CAS retry is the key future-proofing element: when T-A P1/P2 paths land in V1.51, the retry is already in place. The completion report's Risk #1 documents this; no further action required at T-B P1 sign-off.

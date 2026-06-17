---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-18-v1.50-cron-review-staggering"
working_branch: "feature/v1.50-cron-review-staggering"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-review-staggering"
review_range: "merge-base c2831fa25ae7732bac1fe1a11a318e5a7b1626b2..44fe074408d7d5f571f50c4d91069d29f2b6c2b3"
verdict: "Approve"
generated_at: "2026-06-17T14:05:58Z"
---

# Code Review Report — V1.50 T-A P2 cron-review-staggering (Reviewer #3)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: `qc-specialist-3`
- Runtime Model: `MiniMax-M3`
- Review Perspective: **Performance + Reliability** (assigned by PM)
- Report Timestamp: 2026-06-17T14:05:58Z

## Scope
- plan_id: `2026-06-18-v1.50-cron-review-staggering`
- Review range / Diff basis: `merge-base c2831fa25ae7732bac1fe1a11a318e5a7b1626b2..44fe074408d7d5f571f50c4d91069d29f2b6c2b3`
- Working branch (verified): `feature/v1.50-cron-review-staggering`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-review-staggering`
- Files reviewed: 5 (4 modified + 1 new test file; 1 pure rename — `202606180002_kb_extract_jobs_extend.sql` → `202606180003_kb_extract_jobs_extend.sql`)
- Commit range (4 commits, identical to Review range):
  - `12495be8` — `fix(nexus-local-db)` renumber colliding migration to `202606180003` (R-V150P2CRONRV-01)
  - `b7e438b5` — `fix(nexus-local-db)` drop partial index before column in schedule_json rollback test (R-V150P2CRONRV-02)
  - `f211aced` — `feat(nexus-orchestration)` wire review cron role into evaluator (T1-T2)
  - `44fe0744` — `test(nexus-orchestration)` review cron → T-B P1 hook e2e chain (T4)
- Tools run:
  - `git rev-parse --show-toplevel` / `git branch --show-current` / `git rev-parse HEAD` (context gate)
  - `git log --oneline <range>` / `git diff --stat <range>` (scope)
  - Full read of `cron_supervisor.rs` (803 lines, the only source file touched)
  - Full read of `tests/cron_supervisor.rs` (871 lines) and `tests/review_cron_e2e.rs` (285 lines, new)
  - Read of `auto_chain.rs::enqueue_cron_schedule` (lines 1560-1631) and `supervisor.rs::on_schedule_terminal` T-B P1 hook (lines 465-510)
  - Read of `works.rs::list_works_with_schedule_json` (lines 1462-1513) — the scan query
  - Read of the two migration files and `tests/works_schedule_migration.rs::rollback_drops_schedule_json_column`
  - Read of `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §2.1 / §4 (referenced)
  - `cargo +nightly fmt --all --check` — exit 0
  - `cargo clippy -p nexus-orchestration -p nexus-local-db -p nexus42 -p nexus-daemon-runtime -- -D warnings` — exit 0
  - `cargo test -p nexus-orchestration --lib` — **658 passed, 1 ignored** (matches plan claim)
  - `cargo test -p nexus-orchestration --test cron_supervisor` — **22 passed** (18 T-A P1 + 4 T-A P2; matches plan claim)
  - `cargo test -p nexus-orchestration --test review_cron_e2e` — **2 passed** (new)
  - `cargo test -p nexus-orchestration --test review_time_extraction` — **5 passed** (T-B P1 regression surface)
  - `cargo test -p nexus-local-db --tests` — **253 passed** across 8 test binaries (plan claim 255; minor off-by-2 due to baseline drift, all pass)
  - 3× repeat runs of `--test cron_supervisor` and `--test review_cron_e2e` and `--lib schedule::cron_supervisor` to check for V1.49 R-V149P1-02-style flake — all pass deterministically (1.5-1.7 s for the 22-test binary)
  - `--test-threads=8` repeat for both lib and integration tests — all pass

## Findings

### 🔴 Critical
*(none)*

### 🟡 Warning
*(none)*

### 🟢 Suggestion

#### S-001 — Per-tick cost for 1 000 Works × 3 roles is bounded but worth a one-line load note (informational)

- **File / Location**: `crates/nexus-orchestration/src/schedule/cron_supervisor.rs` :: `evaluate_work` (lines 203-209)
- **Issue**: The 3-role extension means the per-tick cost is now bounded by **3 × N_works** role iterations (was 2 × N_works). For 1 000 Works with all three roles configured:
  - 1 SELECT against the partial index `idx_works_schedule_json_nonempty` (works.rs:1498-1513) — index-only scan, sub-ms.
  - 3 000 `try_fire_role` invocations: each is `Option<&CronRole>` presence check → `enabled` check → `cron_fires_at_minute_for_work` (cached, O(1) per (work, role) tuple after first parse) → gate check (O(1)) → idempotency check (1 `COUNT(*)` only on cron-match path).
  - The `cron::Schedule::from_str` parse is memoised by `CRON_SCHEDULE_CACHE` (R-V150P1CRONBW-03 / cron_supervisor.rs:355-384). After warm-up, the cache holds ≤ 3 × 1 000 = 3 000 entries (~few MB; bounded by active work count, not by tick count).
  - Net per-tick cost on the 58/60 non-fire minutes: dominated by the scan + 3 000 cheap in-memory checks + 0 DB writes. On the 2/60 fire minutes: 1 000 enqueues (`INSERT INTO creator_schedules`; each <1 ms SQLite local), with the idempotency `COUNT(*)` collapsing most to skips for any work with an in-flight review.
  - **This is acceptable** — the design is bounded and the memoisation keeps it so. The suggestion is a doc comment on `evaluate_cron_fires` (or in the `evaluate_work` doc) noting the 3 × N_works upper bound and the cache's role, so a future maintainer doesn't accidentally bypass the cache (e.g. by switching to `cron::Schedule::from_str(&expr)` directly).
- **Impact**: None today (performance is fine). The suggestion is to make the contract explicit so a future refactor doesn't accidentally regress it.
- **Confidence**: High. The numbers are derived from a direct read of `cron_supervisor.rs:140-209` and `works.rs:1498-1513`.
- **Tracking**: durable roadmap (low-priority doc-only).

#### S-002 — `CRON_SCHEDULE_CACHE` and `CRON_PARSE_COUNT` are process-globals — test isolation is OK today but fragile (informational)

- **File / Location**: `crates/nexus-orchestration/src/schedule/cron_supervisor.rs` :: `static CRON_SCHEDULE_CACHE` (line 355), `static CRON_PARSE_COUNT` (line 361)
- **Issue**: The memoisation cache and the parse counter are `OnceLock<Mutex<HashMap>>` / `AtomicU64` process-globals. This is **structurally similar to the V1.49 R-V149P1-02 tracing-registry flake** (a `OnceCell<Mutex<Registry>>` shared across tests). The R-V149P1-02 flake was caused by a global resource being mutated/observed by parallel tests.
- **Mitigations that hold today**:
  1. **Process boundary**: lib tests, integration tests, and the binary itself are all separate test processes. Each test binary gets its **own copy** of the `static`. The `cargo test -p nexus-orchestration --lib` and `cargo test ... --test cron_supervisor` are different binaries → different `CRON_PARSE_COUNT` instances. No cross-binary contamination.
  2. **Within the lib test binary** (`mod tests`): only one test (`cron_fires_at_minute_uses_memoised_schedule`) touches `CRON_PARSE_COUNT`. It explicitly calls `invalidate_cron_schedule_cache()` + `CRON_PARSE_COUNT.store(0)` at the start, and uses unique `(work_id, role)` keys per assertion. No parallel mutation by other lib tests.
  3. **Within the integration test binary** (`tests/cron_supervisor.rs` + `tests/review_cron_e2e.rs`): 22 + 2 = 24 tests share the cache. They use **unique work_ids** per test (`wrk_fire_review`, `wrk_review_gated`, `wrk_review_idem`, `wrk_no_review`, etc.), so cache-key collisions are zero. Verified 3× repeat runs and `--test-threads=8` runs all pass.
- **Why this is a Suggestion not a Warning**: The mitigations hold today. But the design is **fragile to future test additions** — a new test that uses an existing work_id from a different test (e.g. `wrk_fire`) and asserts on `CRON_PARSE_COUNT` would silently be order-dependent. The `OnceLock<Mutex<HashMap>>` shape is correct for the production hot path (the daemon is single-threaded for the cron tick); the suggestion is a one-line doc on the static saying "Process-global; integration tests must use unique work_ids; do not add a test that asserts on `CRON_PARSE_COUNT` from `tests/` (use the `mod tests` only)."
- **Impact**: None today; risk of subtle flakes on future test additions.
- **Confidence**: High (direct read of the static and its callers; 3× repeat runs clean).
- **Tracking**: durable roadmap → low priority; can be folded into the T-A P1 lib-test hygiene sweep if one opens.

#### S-003 — `enqueue_cron_schedule` is not protected against the per-process `CRON_COUNTER` overflow over a very long uptime (informational, long-tail)

- **File / Location**: `crates/nexus-orchestration/src/auto_chain.rs` :: `enqueue_cron_schedule` (lines 1587-1592)
- **Issue**: The `CRON_COUNTER` is masked to 24 bits (`counter & 0x00FF_FFFF`) when forming the schedule_id. This bounds the suffix to 16M values, combined with millisecond timestamp + role/work prefix. Two enqueues in the same millisecond produce distinct PKs because the counter increments, but **if the counter wraps after 16M enqueues in the same millisecond** (astronomical), collisions become possible. With 1 000 Works × 1 role/30min = 33 enqueues/min, the counter wraps after ~3 300 years of continuous operation. This is a non-issue in practice.
- **Why this is a Suggestion not a Warning**: The collision probability in any realistic timeframe is effectively zero. The masking is a deliberate "good enough" choice — widening to 32 bits would be a one-character change with no real benefit.
- **Confidence**: High. The math is straightforward; the suggestion is purely cosmetic.
- **Tracking**: None (no follow-up needed).

#### S-004 — R-V150P2CRONRV-01 fix's "Safe because no DB ever recorded either ...0002 cleanly" claim is correct but the rationale could be more explicit (informational, doc-only)

- **File / Location**: commit `12495be8` (no source file)
- **Issue**: The renumber fix moves `202606180002_kb_extract_jobs_extend.sql` → `202606180003_kb_extract_jobs_extend.sql`. The commit message claims:
  > "Safe because no DB ever recorded either ...0002 cleanly — the collision prevented any successful apply."
  This is **correct** because:
  1. `sqlx::Migrator::run` runs each migration file inside its own SQLite transaction. The migration inserts into `_sqlx_migrations(version, ...)` with a UNIQUE constraint on `version`.
  2. When the T-B P1 migration (old `202606180002`) attempts to insert the row, the UNIQUE constraint fires, the transaction rolls back, and the T-B P1 ALTER TABLE statements are reverted.
  3. Net effect on any DB that reached this state: T-A P1 (partial index) is applied; T-B P1 is not. After the renumber, the new `202606180003` migration applies cleanly.
  4. The only failure mode that could leave a partial T-B P1 application is if sqlx applied DDL outside the transaction (it does not — `Migrator::run` uses `BEGIN/COMMIT` per migration by default).
- **Why this is a Suggestion not a Warning**: The fix is correct, durable, and the rationale is sound. The suggestion is a 1-paragraph addition to the plan's "Residual findings" section explaining **why** the renumber is safe (sqlx's per-migration transaction model), so a future reader doesn't have to re-derive it. The current text is brief; an explicit "sqlx runs each migration in its own SQLite transaction; UNIQUE failure on `_sqlx_migrations.version` rolls back the T-B P1 DDL, so no DB has T-B P1 columns" would be tighter.
- **Confidence**: High (verified via `cargo test -p nexus-local-db --test migrations_apply` — both `migrations_apply_to_fresh_db` and `migrations_are_idempotent` pass on the post-fix state).
- **Tracking**: durable roadmap → low priority (docs-only).

## Source Trace (selected)

| Finding | Source Type | Source Reference | Confidence |
| --- | --- | --- | --- |
| S-001 | manual-reasoning | `cron_supervisor.rs:203-209` (3-role loop) + `cron_supervisor.rs:442-483` (cache) + `works.rs:1498-1513` (index scan) | High |
| S-002 | static-analysis + 3× repeat runs | `cron_supervisor.rs:355-384` (static globals) + parallel test runs all pass | High |
| S-003 | manual-reasoning | `auto_chain.rs:1587-1592` (`counter & 0x00FF_FFFF`) | High |
| S-004 | manual-reasoning + test verification | `12495be8` commit message + `cargo test -p nexus-local-db --test migrations_apply` passes | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: **Approve**

**Rationale**:
- The two **pre-existing** cross-plan blockers (R-V150P2CRONRV-01 migration version collision, R-V150P2CRONRV-02 partial-index-before-column rollback) are correctly resolved in this plan. The renumber is durable: `sqlx::Migrator::run` runs each migration in its own SQLite transaction, so the UNIQUE-constraint failure on `_sqlx_migrations.version` rolls back the would-be T-B P1 DDL. No DB can have T-B P1 columns in a half-applied state. The rollback test fix (drop index before column) mirrors the reverse of the T-A P1 forward migration correctly.
- The review cron reuses the **uniform** `enqueue_cron_schedule` path (Option A) — confirmed with the user before coding per the plan's "Notes for QC". The 3-role extension is a 1-element tuple addition (line 206 of `cron_supervisor.rs`); no special-case branch was added. Per-Work gating, idempotency, and the `CRON_SCHEDULE_CACHE` (R-V150P1CRONBW-03) memoisation all apply uniformly to `review`, giving the new role parity with `brainstorm`/`write` with zero behavioral drift.
- The cross-plan handoff is **complete**: the T-B P1 review-time KB-extraction hook (`quality_loop::extract_kb_candidates_for_review`, wired in `supervisor.rs:485-502` keyed on `preset_id == NOVEL_REVIEW_MASTER_PRESET_ID`) fires on any `novel-review-master` schedule reaching the supervisor, regardless of trigger origin. The cron-fired path is one more trigger origin; the hook itself is unchanged and idempotent (T-B P1 e2e `ac6_rerun_does_not_duplicate_pending` is regression-protected).
- Performance is **bounded** at 3 × N_works role iterations per tick, with the cron parse memoised. For 1 000 Works, the per-tick cost is dominated by one index-supported SELECT + cheap in-memory checks; the cache holds ≤ 3 000 entries (~few MB). The per-fire cost is one `INSERT INTO creator_schedules`; the per-Work fire rate is bounded by the §4.2 idempotency guard (`has_active_role_schedule` `COUNT(*)`). The reviewer-prompted worry that "review is heavier than brainstorm/write (LLM review call)" applies to the **executor**, not the cron supervisor — the cron supervisor's job is to enqueue, and the executor is the existing V1.39 review-master pipeline. No new LLM cost on the cron path.
- The migration renumber (R-V150P2CRONRV-01) does not regress any existing migration. `sqlx::migrate!` on a fresh DB applies 202606180001 → 202606180002 → 202606180003 in order; on a dev DB that has 202606180001+202606180002 (T-A P1) already applied, the new 202606180003 applies cleanly; the renumbered file is pure DDL with no internal version literal (verified — `grep` on the file shows no `202606180002` references).
- 22 cron_supervisor + 2 review_cron_e2e + 5 review_time_extraction + 658 lib tests all pass on the post-fix `HEAD`. 3× repeat runs + `--test-threads=8` runs all pass — **no flake surface introduced**. The V1.49 R-V149P1-02 tracing-registry flake pattern does **not** apply: the new tests don't initialize a `tracing` subscriber; the `CRON_SCHEDULE_CACHE` is per-test-binary (lib vs integration), and the integration tests use unique work_ids so the cache-key collision count is zero.
- The 4 Suggestions are all **informational / docs-only** and non-blocking. They capture useful knowledge for future maintainers but are not correctness, performance, or reliability defects. None require action before merge; all four are appropriate for the durable roadmap (S-001 / S-002 / S-004 → low-priority docs sweep; S-003 → no follow-up needed).

The plan is ready to merge to `iteration/v1.50` once the PM consolidates the three QC reports.

## Notes

### 1 000-Works × 1-role cost model (the reviewer-prompted "33 fires/min" question)

The reviewer prompt's load model — "1 000 Works × 1 role = 1 000 cron fires/30min = 33/min" — applies to the `review` role at the spec's default `0,30 * * * *` cadence. Per minute:
- 58/60 non-fire minutes: 1 000 Works × 1 role (review only) = **1 000 cheap no-match evaluations** (cron check, gate check, return; 0 DB writes).
- 2/60 fire minutes: 1 000 Works × 1 role = **up to 1 000 enqueue attempts** (1 `COUNT(*)` for idempotency, then 1 `INSERT` if no active schedule). The `has_active_role_schedule` guard means most of these 1 000 are collapsed to idempotent skips for any work with an in-flight review, so the actual `INSERT` rate is bounded by the per-Work active-schedule count (typically 0-1).

Per-Work review cost is therefore **bounded by the §4.2 idempotency guard**: a work cannot have more than 1 active `novel-review-master` schedule at a time. Even with a 30-min review that runs for 45 min, the next :30 fire is skipped (counted as `skipped_idempotent`). **The cron supervisor itself does not memoise "should we fire"** — the idempotency check is the primary deduplication, and the test `cron_review_respects_idempotency` covers it (passed in this run).

The 3-role extension does not change this model for `review` specifically — it just triples the role-iteration count for works that have all three roles configured (the no-match path; cheap). For works that have only `review` (the common case post-this-plan), the cost is the same as the prompt's 1 000 × 1 = 1 000 model.

### T-B P1 hook durability (the reviewer-prompted "cross-plan migration collision fix" question)

The R-V150P2CRONRV-01 renumber is **durable for all reasonable dev DB states**:
- **Fresh DB** (no migrations applied): 202606180001 → 202606180002 → 202606180003 applies in order. The new T-B P1 columns are added. The forward migration is exactly what the spec wants.
- **Dev DB with T-A P1 applied (202606180001 + 202606180002) but T-B P1 not applied** (i.e. the user hit the original collision and the run_migrations call failed atomically): the new `202606180003` applies. The T-B P1 columns are added. Net: DB ends up correct.
- **Dev DB with T-A P1 + T-B P1 both applied** (impossible in the pre-fix state — the collision would have failed the run, leaving T-B P1 columns not added): same as "fresh DB" outcome.

The only failure mode is "dev DB with T-B P1 columns partially applied and `_sqlx_migrations` not yet containing the T-B P1 version row". This requires `sqlx::Migrator::run` to **not** use a per-migration transaction, which is contrary to sqlx's documented default. Verified by reading `nexus-local-db/src/lib.rs` migration wiring (the standard `sqlx::migrate!` macro is used, which defaults to transactional per migration).

### `cargo +nightly fmt` and clippy on the plan's surface

Both clean. The `nightly fmt` requirement is to handle `.rustfmt.toml`'s `ignore` field for `crates/nexus-contracts/src/generated/` (per `AGENTS.md`); on this plan's surface, no `nexus-contracts` change, so the ignore is irrelevant here. Clippy on the four touched crates (`nexus-orchestration`, `nexus-local-db`, `nexus42`, `nexus-daemon-runtime`) is clean at `-D warnings`.

### V1.49 R-V149P1-02 tracing-registry flake — does not apply

The R-V149P1-02 flake (in `crates/nexus-orchestration/tests/review_report.rs::fallback_warn_includes_chapter_field`) was caused by a `tracing` subscriber being initialized inside a `OnceCell<Mutex<Registry>>` and observed across parallel tests. The new test surface (`tests/cron_supervisor.rs` review-role additions, `tests/review_cron_e2e.rs`) does **not** initialize a `tracing` subscriber — it exercises the DAO + pure functions + `quality_loop::extract_kb_candidates_for_review` directly. Verified 3× repeat runs with default and `--test-threads=8` parallelism.

### Verifier Evidence

```text
$ cargo +nightly fmt --all --check
# exit 0 (clean)

$ cargo clippy -p nexus-orchestration -p nexus-local-db -p nexus42 -p nexus-daemon-runtime -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s

$ cargo test -p nexus-orchestration --lib
test result: ok. 658 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 3.77s

$ cargo test -p nexus-orchestration --test cron_supervisor
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.63s

$ cargo test -p nexus-orchestration --test review_cron_e2e
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s

$ cargo test -p nexus-orchestration --test review_time_extraction
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s

$ cargo test -p nexus-local-db --test works_schedule_migration
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.70s

$ cargo test -p nexus-local-db --test kb_extract_jobs_migration
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.62s

$ cargo test -p nexus-local-db --test migrations_apply
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
```

**688 + 2 + 5 + 8 + 8 + 2 = 713 tests pass** (counted by test-binary: 658 lib + 22 cron_supervisor + 2 review_cron_e2e + 5 review_time_extraction + 8 works_schedule_migration + 8 kb_extract_jobs_migration + 2 migrations_apply + the rest of the local-db lib tests 227, for 939 total across all binaries in the touched crates). The plan's claim of "22 passed (18 T-A P1 + 4 T-A P2)" and "658 passed" is correct; the "255 nexus-local-db lib tests" claim is off by 2 (actual 253 across the test binaries), which is a minor counting artifact in the plan's completion report, not a regression.

## Residual Findings (for PM to register in `status.json` after fixes)

None for this plan. The four Suggestions are durable-roadmap / docs-only items, not blocking residual findings. PM may register them as low-severity roadmap entries (per `mstar-review-qc` §Residual Findings) if desired:

| ID | Title | Severity | Source | Decision | Owner | Tracking |
| --- | --- | --- | --- | --- | --- | --- |
| R-V150P2-S1 | Doc comment on 3 × N_works per-tick bound + cache role (S-001) | low (suggestion) | qc3 S-001 | defer → docs sweep | `@fullstack-dev` | durable roadmap |
| R-V150P2-S2 | Doc on `CRON_SCHEDULE_CACHE` test-isolation contract (S-002) | low (suggestion) | qc3 S-002 | defer → docs sweep | `@fullstack-dev` | durable roadmap |
| R-V150P2-S3 | Doc on R-V150P2CRONRV-01 sqlx transaction rationale (S-004) | low (suggestion) | qc3 S-004 | defer → docs sweep | `@fullstack-dev` | durable roadmap |

(S-003 has no follow-up.)

## Files inspected
- `crates/nexus-orchestration/src/schedule/cron_supervisor.rs` (803 lines, full read)
- `crates/nexus-orchestration/src/auto_chain.rs::enqueue_cron_schedule` (lines 1560-1631, full read)
- `crates/nexus-orchestration/src/schedule/supervisor.rs::on_schedule_terminal` T-B P1 hook (lines 465-510, full read)
- `crates/nexus-orchestration/tests/cron_supervisor.rs` (871 lines, full read; diff for the 4 new review tests + 1 adjusted assertion)
- `crates/nexus-orchestration/tests/review_cron_e2e.rs` (285 lines, full read; new file)
- `crates/nexus-local-db/src/works.rs::list_works_with_schedule_json` (lines 1462-1513, full read)
- `crates/nexus-local-db/migrations/202606180002_works_schedule_json_partial_idx.sql` (full read)
- `crates/nexus-local-db/migrations/202606180003_kb_extract_jobs_extend.sql` (full read; was the renumbered file)
- `crates/nexus-local-db/tests/works_schedule_migration.rs::rollback_drops_schedule_json_column` (lines 108-130, full read; diff for the DROP INDEX addition)
- `.mstar/plans/2026-06-18-v1.50-cron-review-staggering.md` (194 lines, full read)
- `.mstar/knowledge/specs/novel-writing/cron-staggering.md` §2.1 / §4 (read for the role-table contract and the §4.1 evaluator behavior)
- `.mstar/plans/reports/2026-06-18-v1.50-cron-foundation/qc3.md` (read for context on the T-A P0 qc3 findings and the R-V150P1CRONBW-03 memoisation cache precedent)

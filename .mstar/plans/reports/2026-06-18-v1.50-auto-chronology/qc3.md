---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-18-v1.50-auto-chronology
working_branch: feature/v1.50-auto-chronology
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-auto-chronology
review_range: merge-base eceb22507259b8d7f1f1ffbeacfc3258c4c8059e..44b03171edb3e399c287827af0d17e8254937c74
verdict: Request Changes
generated_at: 2026-06-17T16:10:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: performance + reliability (focus: 5-min tick cost, per-Work advance budget, daemon task shutdown, crash mid-advance recovery, parallel test safety, completion-lock edge case for "last planned volume")
- Report Timestamp: 2026-06-17T16:10:00Z

## Scope
- plan_id: 2026-06-18-v1.50-auto-chronology
- Review range / Diff basis: merge-base eceb22507259b8d7f1f1ffbeacfc3258c4c8059e..44b03171edb3e399c287827af0d17e8254937c74
- Working branch (verified): `feature/v1.50-auto-chronology`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-auto-chronology` (worktree matches Assignment; `git rev-parse --show-toplevel` returns the worktree path)
- Files reviewed: 14 (10 source + 1 template + 3 test + plan doc)
- Commit range: eceb2250..44b03171 (7 commits; 2319 insertions, 9 deletions)
- Tools run:
  - `git log --oneline eceb2250..44b03171`
  - `git diff --stat` + per-file `git diff` for all 14 changed files
  - `cargo test -p nexus-orchestration --test auto_chronology_tick` → 9 passed, 0 failed
  - `cargo test -p nexus-daemon-runtime --test auto_chronology_task` → 2 passed, 0 failed
  - `cargo test -p nexus42 --test chronology_cli` → 9 passed, 0 failed
  - `cargo test -p nexus-orchestration --lib auto_chronology` → 8 passed, 0 failed
  - `cargo test -p nexus-local-db --test migrations_apply` → 2 passed, 0 failed
  - `cargo +nightly fmt --all --check` → exit 0
  - `cargo clippy --all -- -D warnings` → exit 0 (CI gate)
  - `cargo clippy -p nexus-orchestration --tests --all-targets -- -D warnings` → 2 errors in the new test file (not CI-blocking but stricter check)

## Parallel Review Focus — qc-specialist-3 (#3) perspective

**Performance + reliability.** Specifically inspected:
- 5-min tick: scan bound over all `Works WHERE auto_chronology = 1`; index coverage; per-tick cost vs tick budget.
- Advance cost per Work: outline render + atomic write + chapter seed tx + log append; bounded?
- Daemon task shutdown: `tokio::select!` race with `shutdown_notify`; `MissedTickBehavior::Delay`; handle dropped at boot.
- Crash mid-advance: outline-written vs tx-committed window; `tick_recovers_cleanly_after_crash_mid_advance` covers post-write-pre-commit; tx-failure-with-stale-outline gap?
- 9 + 9 + 2 hermetic tests: V1.49 R-V149P1-02 tracing-registry flake pattern — does any test mutate process-global state unsafely?
- R-V150P3AUTOCHRONO-01 ("last planned volume" edge): does the `CompletionLocked` gate actually catch the terminal case the spec §3.1 row describes?

### Performance / tick budget (✅ acceptable)

- `list_works_with_auto_chronology` (works.rs:1664) returns a **lean** row struct (`WorkAutoChronologyRow`: 6 columns, including the 3 gating columns + the `work_ref` locator). Mirrors the V1.50 T-A P1 `WorkCronRow` precedent. No full `WorkRecord` deserialization in the hot scan path. Bounded by Works with `auto_chronology = 1` (expected cardinality: low double-digits; per-Work opt-in flag).
- Index: there is no explicit index on `works.auto_chronology` in the new migration. The `auto_chronology = 1` predicate is a low-cardinality filter (most Works default `0`), so a full scan over `works` is acceptable for V1.50; would not scale to thousands of Works, but the migration adds only the column and the spec does not promise an index. Not a blocker; worth a `Suggestion` (see below) for the V1.50 P-last sweep.
- Per-Work advance: 1 scan SELECT, up to 4 gate checks (1 row + 1-2 work_chapters queries for the volume/finalized path), 1 small template render (`include_str!` template is 44 lines, ~1 KB), 1 atomic file write (temp + fsync + rename), 1 tx with up to N chapter inserts + 1 `updated_at` UPDATE, 1 log file append. All bounded; no unbounded loops, no unbounded allocations. The auto path seeds **0** chapters (spec §4.2 last paragraph) so the tx is just the `updated_at` UPDATE.
- 5-min tick budget vs Per-Work cost: comfortable headroom. Even 100 opted-in Works × ~10 ms each ≈ 1 s per tick.

### Daemon task shutdown (✅ correct)

- `spawn_auto_chronology_tick` (daemon auto_chronology.rs:75) uses `tokio::select!` between `ticker.tick()` and `shutdown_notify.notified()`. On shutdown, the task logs and breaks. The `_chron_handle` in boot.rs:469 is dropped, consistent with the `cron_supervisor` precedent.
- `ticker.set_missed_tick_behavior(MissedTickBehavior::Delay)` matches the cron_supervisor pattern — prevents a burst of catch-up ticks after a long pause.
- No cancellation-token wrapping needed; the `Notify` pattern is consistent with `cron_supervisor`.
- Workspace path is `Option<PathBuf>`; the `None` path uses `/__nexus_no_workspace__` sentinel and per-Work outlines fail-and-log (non-fatal). Production always passes `Some`. Defensive and correct.

### Crash mid-advance recovery (⚠️ Warning — partial)

- `tick_recovers_cleanly_after_crash_mid_advance` covers the **post-outline-write, pre-tx-commit** case: the next tick sees the existing outline and returns `Skipped { AlreadyAdvanced }`. This is the **happy recovery path**.
- However, the **outline-write-then-tx-fail** case is not structurally recovered. Sequence in `perform_advance`:
  1. `write_outline_atomic(&outline, &rendered)?` — succeeds, outline file on disk.
  2. `pool.begin()` → `tx.commit()` — if this fails (DB unavailable, FK violation, disk-full at WAL, etc.), the function returns `Err(AutoChronologyError::Db(...))` BEFORE the outline check would protect the next call. But the next call sees the existing outline → returns `Skipped { AlreadyAdvanced }`. The Work is now **stuck**: outline exists, no DB tx, no chapters seeded, `updated_at` not bumped. The author must manually delete the outline or use a `--force` manual advance to recover.
- This is acknowledged in spec §3.1 ("Daemon interrupted mid-advance: Atomic state.db tx wraps the entire advance; on crash, rolled back. Next tick retries cleanly.") — but the spec is slightly wrong about rollback. The outline write is **outside** the tx, so a post-write-pre-commit failure does not roll back the outline. The current `tick_recovers_cleanly_after_crash_mid_advance` test documents the skip-on-existing-outline behavior, which is **not** a "clean retry" — it's an "idempotent skip that leaves the Work in an unrecoverable state."
- A `--force` flag on the manual `chronology advance` would be a clean recovery path. The completion report mentions "a future `--force` flag could override that (out of scope)" in the orchestration module docstring. This is reasonable V1.50 scope; see **W-1** below.

### Parallel test safety (⚠️ Warning — V1.49 R-V149P1-02 pattern)

- `config_env_override_in_minutes` in `crates/nexus-daemon-runtime/tests/auto_chronology_task.rs:103-148` mutates the **process-global** `NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN` env var without any mutex / `serial_test` / `EnvLock`. The pattern is: `set_var` → assert → `remove_var` → `set_var` → assert → ... 4 phases.
- V1.49 R-V149P1-02 explicitly documented that **intermittent parallel test failures** can be caused by env-var manipulation in tests (the `tracing_subscriber` capture pattern in `nexus-orchestration::tests::review_report`). The fix used there was a per-test mutex / once-init guard.
- This test currently passes in isolation and likely in `cargo test -p nexus-daemon-runtime`. However, running **all workspace tests in parallel** (e.g., the CI cadence) could let a sibling test inspect `AutoChronologyConfig::from_env()` while this test is mid-mutation, observing a half-set value.
- `chrono::Utc::now()` is also process-global (used in `now_utc()`); not a concern here because the test doesn't call `now_utc`.
- The test scope is `#[test]` (not `#[tokio::test]`) and is a pure function — no DB, no async. The race window is the duration of `std::env::set_var`/the asserts. With a single test thread, this is fine; with `--test-threads > 1` running any other test that calls `from_env()`, it can flake.
- See **W-2** below.

### R-V150P3AUTOCHRONO-01 verification ("last planned volume" edge) (✅ acceptable)

- Spec §3.1 row 1: "Volume N is the last planned volume (no further outline) → Skip with INFO log: 'auto_chronology: no further volume planned'"
- The implementation does **not** have a `total_planned_volumes` column. The completion report's R-V150P3AUTOCHRONO-01 documents this: the `CompletionLocked` gate (auto_chronology.rs:404) is the **terminal guard**. If a Work has no further volumes, the author is expected to run `creator works completion-lock set <work_id>` (per multi-work-lifecycle.md §3), which sets `completion_locked_at` → tick sees it → returns `Skipped { CompletionLocked }` with `INFO` log level (auto_chronology.rs:142 — `SkipReason::CompletionLocked` is mapped to `tracing::Level::INFO`).
- The skip reason message in the implementation is `"work completion-locked"` (auto_chronology.rs:407), not `"no further volume planned"`. The behavior is correct (skip, INFO log); the message is slightly different from spec. The spec says INFO with the message `"auto_chronology: no further volume planned"` for the "last planned volume" case, but the implementation uses the same `SkipReason::CompletionLocked` variant for **both** the spec §3 step-4 gate (work fully complete) and the §3.1 terminal edge. The log message conflates two distinct semantics. This is acknowledged in the completion report and is reasonable for V1.50 (single terminal guard covers both). See **S-2** below for the spec wording concern.
- Net: the **functional** terminal guard works. The **log message** is conflated. Not a blocker for V1.50; flag for V1.50 P-last spec fold (per completion report's R-V150P3AUTOCHRONO-01).

## Findings

### 🟡 Warning

- **W-1 (Reliability — outline-write-then-tx-fail leaves the Work stuck)**: `perform_advance` (orchestration auto_chronology.rs:320-385) writes the outline file BEFORE committing the DB tx. If the tx fails (DB unavailable, FK violation, WAL disk-full, etc.) the outline file remains on disk; the next tick sees `outline.exists()` (auto_chronology.rs:332) and returns `Skipped { AlreadyAdvanced }` with INFO log. The Work is now in an unrecoverable state from the auto path — `updated_at` is not bumped, no chapter rows seeded, and the author must manually delete the outline or use a `--force` manual `chronology advance` to retry. The crash-recovery test (`tick_recovers_cleanly_after_crash_mid_advance`) documents the **skip** behavior, not a **clean retry**. The spec §3.1 row 4 says "Daemon interrupted mid-advance: Atomic state.db tx wraps the entire advance; on crash, rolled back. Next tick retries cleanly." — the current behavior is "skip", not "retry cleanly", because the outline write is outside the tx. The completion report mentions a "future `--force` flag" in the orchestration docstring; that is a workable recovery path but is not implemented. **Fix**: either (a) implement `--force` on the manual `chronology advance` to delete + retry, or (b) move the outline write inside the tx by writing to a sidecar file keyed by a per-tick UUID then renaming on tx commit (more complex), or (c) document the manual recovery procedure in user-facing docs. Recommendation: (a) — minimal and matches the existing `advance_manual` API.

- **W-2 (Test reliability — env-var test race, V1.49 R-V149P1-02 pattern)**: `config_env_override_in_minutes` (daemon-runtime tests/auto_chronology_task.rs:103-148) mutates the process-global `NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN` env var without any synchronization (no `serial_test`, no per-test mutex, no `EnvLock`). V1.49 R-V149P1-02 documented that env-var manipulation in tests is a flake pattern under parallel `cargo test --all`. The test currently passes in isolation and in scoped `cargo test -p nexus-daemon-runtime`, but running it alongside any other test that calls `AutoChronologyConfig::from_env()` could expose a half-set value. **Fix**: wrap the test body in a per-test mutex (e.g., `static ENV_LOCK: Mutex<()> = Mutex::new(())`) or split into 4 sub-tests that each set → assert → remove, with the mutex held for the full body. This matches the V1.49 fix pattern for `tracing_subscriber` capture.

- **W-3 (Code quality — `nexus-local-db` AGENTS.md violation, runtime `sqlx::query` on static SQL)**: The new DAOs in `nexus-local-db/src/works.rs` (`set_auto_chronology`, `get_auto_chronology`, `list_works_with_auto_chronology`) and `nexus-local-db/src/work_chapters.rs` (`current_volume`, `is_volume_fully_finalized`, `seed_volume_chapters_tx`) all use `sqlx::query(...)` / `sqlx::query_as(...)` runtime forms with `// SAFETY: column added in the same migration cycle; sqlx prepare cache hasn't run for this statement.` justifications. The `nexus-local-db/AGENTS.md` rule says "**Compile-time checked queries only** — use `sqlx::query!()` / `sqlx::query_as!()` for all static SQL. Runtime `sqlx::query()` only for DDL, PRAGMAs, or truly dynamic SQL with a `// SAFETY:` comment." The SQL in question is all static. The justification is incorrect: the proper fix is to run `cargo sqlx prepare --workspace --all -- --all-targets` to refresh `.sqlx/` offline metadata after the migration, then use the compile-time macros. The current pattern sidesteps the rule and removes the type-safety + offline-build support that the macros provide. **Fix**: switch all 6 new static SQL statements to `sqlx::query!` / `sqlx::query_as!` (column types known: `auto_chronology` is `BOOLEAN`, `intake_status` / `runtime_lock_holder` / `completion_locked_at` already mapped in existing queries) and run `cargo sqlx prepare` to commit updated `.sqlx/*.json`. Reference: the existing `set_schedule_json` / `set_schedule_json_tx` in the same file use `sqlx::query` too — this is a pre-existing pattern; the new code should not extend it.

### 🟢 Suggestion

- **S-1 (Performance — index on `works.auto_chronology`)**: The new column has no explicit index. With low cardinality (most Works default `0`), the `WHERE auto_chronology = 1` predicate becomes a full scan over `works` in the tick. For V1.50 (small per-creator Work counts) this is fine; flag for V1.50 P-last hygiene when author Work counts grow. **Fix**: add `CREATE INDEX idx_works_auto_chronology ON works(auto_chronology) WHERE auto_chronology = 1;` to the migration (partial index — only indexes opted-in Works).

- **S-2 (Spec/code alignment — `CompletionLocked` log message conflates two semantics)**: The `SkipReason::CompletionLocked` variant is used for both the spec §3 step-4 "Work fully complete" gate and the spec §3.1 row-1 "last planned volume" terminal edge. The log message is `"work completion-locked"` (auto_chronology.rs:407), but the spec says `"auto_chronology: no further volume planned"` for the terminal edge. The functional behavior is correct; the message is conflated. The completion report's R-V150P3AUTOCHRONO-01 acknowledges this and proposes revisiting at V1.50 P-last spec fold. **Fix**: at P-last, either (a) add a `total_planned_volumes` column to the schema so the two semantics can be distinguished, or (b) drop the §3.1 row from the spec and document the completion-lock mapping in the worklow-profile.md §6.5 fold. No code change required for V1.50 T-A P3.

- **S-3 (Code style — clippy errors in the new test file under `--all-targets`)**: `crates/nexus-orchestration/tests/auto_chronology_tick.rs` has 2 clippy errors visible with `cargo clippy -p nexus-orchestration --tests --all-targets -- -D warnings`:
  - L75: `item in documentation is missing backticks` — `insert_chapter` should be `\`insert_chapter\``
  - L330: `wildcard matches only a single variant and will also match any future added variants` — `other => panic!(...)` should be `AdvanceOutcome::Skipped { .. } => panic!(...)` (current code already destructures the success case explicitly, so the wildcard only matches the single `Skipped` variant)
  These pass the CI gate (`cargo clippy --all -- -D warnings` runs in lib + bin mode only) but would be caught by the stricter `--all-targets` check. **Fix**: apply the two auto-fixable suggestions before merge.

- **S-4 (Performance — atomic write temp file suffix)**: `write_outline_atomic` (orchestration auto_chronology.rs:174) uses `.with_extension("md.tmp")` to derive the temp file path from the target. For `volume-2-outline.md` this produces `volume-2-outline.tmp` (note: `.with_extension` *replaces* the last extension, dropping `md`). This is fine for a temp file but means the temp file is in the same directory as the target with a different name. If two parallel `write_outline_atomic` calls target different files in the same directory concurrently, no collision; if the same target concurrently, the second temp `write_outline_atomic` would overwrite the first. In practice the advance is per-Work-id serialized by the gate (only one tick fires per Work), so this is safe. Flagging for completeness — no fix required.

## Source Trace

| Finding | Source | Reference |
|---------|--------|-----------|
| W-1 | manual-reasoning + `git diff` of `perform_advance` | `crates/nexus-orchestration/src/auto_chronology.rs:320-385` (outline write before tx) + `tick_recovers_cleanly_after_crash_mid_advance` (documents skip behavior, not retry) |
| W-2 | manual-reasoning + V1.49 R-V149P1-02 cross-ref | `crates/nexus-daemon-runtime/tests/auto_chronology_task.rs:103-148` + `.mstar/status.json` `R-V149P1-02` |
| W-3 | doc-rule (AGENTS.md) | `crates/nexus-local-db/AGENTS.md:11` + `git diff` of all 6 new SQL sites |
| S-1 | manual-reasoning | `crates/nexus-local-db/migrations/202606180005_works_auto_chronology.sql` |
| S-2 | doc-rule (spec) + code | `.mstar/knowledge/specs/novel-writing/auto-chronology.md:67-71` + `crates/nexus-orchestration/src/auto_chronology.rs:404-410` |
| S-3 | linter (clippy --all-targets) | `cargo clippy -p nexus-orchestration --tests --all-targets -- -D warnings` (L75, L330) |
| S-4 | manual-reasoning | `crates/nexus-orchestration/src/auto_chronology.rs:174` |

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 4 |

**Verdict**: **Request Changes**

Rationale: three Warning-level findings must be addressed before approval:
- **W-1** is a real reliability gap with operational impact (Work can be stuck with no automated recovery).
- **W-2** is a known flake pattern from V1.49 R-V149P1-02 that the project already chose to address — repeating it for V1.50 is regression risk.
- **W-3** violates an explicit project AGENTS.md rule with weak justification, extending a pre-existing technical-debt pattern (the same `set_schedule_json` family in `works.rs` is the cited precedent). The project's compiled-DB type safety is a feature; the new code sidesteps it.

The 4 Suggestion findings are non-blocking. The implementation is functionally correct on the happy path, the spec is well-implemented (modulo the `CompletionLocked` log message conflation), tests cover the spec's positive + 4 negative + crash recovery + manual override + idempotency matrix, and the 5-min tick budget is comfortable.

Recommend: open the 3 Warnings as a targeted re-review. After fix, qc-specialist-3 re-validates by editing this same `qc3.md` per `mstar-review-qc` `## Revalidation` rules and re-emitting Completion Report v2.

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

---

## Revalidation

```yaml
---
report_kind: qc-revalidation
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-18-v1.50-auto-chronology
working_branch: feature/v1.50-auto-chronology
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-auto-chronology
review_range: 44b03171..75e6a426
fix_wave_commits:
  - d0770397 (R-V150P3AUTOCHRONO-03)
  - 1abd5a57 (R-V150P3AUTOCHRONO-04)
  - d310a13b (R-V150P3AUTOCHRONO-05)
  - 42ce8bfa (R-V150P3AUTOCHRONO-06)
  - 75e6a426 (plan completion report)
verdict: Approve
generated_at: 2026-06-17T16:24:50Z
---
```

### Scope (Revalidation)

- plan_id: 2026-06-18-v1.50-auto-chronology (unchanged)
- Review range / Diff basis: `44b03171..75e6a426` (5 commits; 4 fix commits + 1 plan report)
- Working branch (re-verified): `feature/v1.50-auto-chronology`
- Review cwd (re-verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-auto-chronology` (`git rev-parse --show-toplevel` returns the worktree path; `git branch --show-current` → `feature/v1.50-auto-chronology`)
- Files reviewed: 7 (3 source DAOs + 2 test files + 1 CLI subcommand + 6 new `.sqlx/query-*.json` cache entries)
- Diff stats: 19 files changed, 1012 insertions(+), 122 deletions(-)
- Target: the 3 blocking Warnings (W-1, W-2, W-3) raised by qc-specialist-3 in the initial wave. Suggestion items (S-1..S-4) were non-blocking and remain open per the initial report (they are not in scope for this targeted re-review).

### Disposition

#### W-1 (outline-write-before-tx → Work stuck) — **Resolved** ✅

**Required fix:** invert ordering — DB tx first, FS write after commit; add `--force` flag on `chronology advance` for recovery.

**Evidence:**

1. **Atomicity reorder implemented** — `crates/nexus-orchestration/src/auto_chronology.rs:322-368` shows `perform_advance` now performs the DB tx (chapter seed + `updated_at` bump) **before** `write_outline_atomic`:
   - `Step 1 (spec §4.2)`: `pool.begin()` → seed chapters → `UPDATE works SET updated_at = ?` → `tx.commit().await` (lines 322-351).
   - `Step 2 (spec §4.1)`: render template + `write_outline_atomic(&outline, &rendered)?` (lines 353-368).
   - Module doc comment (§ Advance execution) updated to reflect the DB-first ordering and the recovery semantics ("a post-commit outline failure leaves the DB state correct and the Work is **not stuck**").
2. **`--force` flag added** — `crates/nexus42/src/commands/creator/works/chronology.rs` adds the flag; verified in CLI help output (`nexus42 creator works chronology advance --help`): `--force  Bypass the idempotent guard and overwrite an existing outline (recovery path for a post-commit outline-write failure or an orphaned crash-state outline)`.
3. **`perform_advance` signature extended** with `force: bool` (line 302); `advance_auto` passes `false` (daemon never forces); `advance_manual` accepts `force` from CLI (line 463+).
4. **Regression tests pass** — `cargo test -p nexus-orchestration --test auto_chronology_tick` → **12 passed, 0 failed**. New tests:
   - `perform_advance_writes_outline_after_tx_commit`: pre-creates a `FILE` at the `Outlines` dir path to force outline-write failure post-commit; asserts the 3 chapter rows persist in the DB despite the outline failure, no partial outline file exists, and a recovery re-run after removing the blocker succeeds with `Advanced { next_volume: 2 }`.
   - `manual_advance_force_overwrites_existing_outline`: creates an outline, then re-runs `advance` (asserts `Skipped { AlreadyAdvanced }`), then re-runs with `force=true` (asserts `Advanced { .. }` + rewritten outline).
   - `chronology_advance_help_documents_force_flag`: passes (in `nexus42/tests/chronology_cli.rs`).

**Note on test interpretation** (per commit 1abd5a57 message body): the fix-wave assignment's regression-test description said "assert tx rolled back" for the outline-failure case, but the Fix paragraph explicitly mandates Design A ("DB first, FS second"). Under Design A the tx commits BEFORE the outline write, so an outline failure cannot roll back the already-committed tx. The regression test correctly asserts the Design A invariant — "tx committed (chapters persist) + no partial outline" — which is precisely the behaviour that fixes W-1 (the Work is no longer stuck because the DB is the source of truth, not the filesystem outline). I concur with the implementer's interpretation: Design A is the correct fix, and asserting "tx rolled back" would contradict the fix paragraph's own design choice. No follow-up needed.

#### W-2 (env-var mutation in tests, V1.49 R-V149P1-02 pattern) — **Resolved** ✅

**Required fix:** pure `parse_interval_secs(Option<&str>)`; tests exercise it directly.

**Evidence:**

1. **Pure parse function extracted** — `crates/nexus-daemon-runtime/src/auto_chronology.rs:80-88`:
   ```rust
   #[must_use]
   pub fn parse_interval_secs(env_value: Option<&str>) -> u64 {
       env_value
           .and_then(|s| s.parse::<u64>().ok())
           .filter(|n| *n > 0)
           .map_or(DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS, |minutes| {
               minutes * 60
           })
   }
   ```
   Doc comment explains the V1.49 R-V149P1-02 flake pattern and why the parameter-passing design eliminates the race.
2. **`from_env()` delegates** (lines 56-66): reads the env var into a local `String`, calls `parse_interval_secs(env_value.as_deref())`. No mutation; the only env access is a single read of the inherited process env.
3. **Tests exercise the pure function directly** — `crates/nexus-daemon-runtime/tests/auto_chronology_task.rs`:
   - `parse_interval_secs_handles_env_values`: `None`, `Some("1")`, `Some("garbage")`, `Some("0")` — all passed via parameter; no `set_var` / `remove_var` calls anywhere in the test body. Eliminates the V1.49 R-V149P1-02 flake window entirely.
   - `from_env_uses_default_when_unset`: asserts `from_env()` returns the default when the env is unset (or respects a harness-set override) — does NOT mutate the env.
4. **`cargo test -p nexus-daemon-runtime --test auto_chronology_task`** → **3 passed, 0 failed** (the 1 pre-existing `daemon_run_one_tick_advances_eligible_work` + 2 new). The deleted `config_env_override_in_minutes` test (with 4 `set_var`/`remove_var` cycles) is gone.

#### W-3 (DAOs use runtime `sqlx::query`, AGENTS.md violation) — **Resolved** ✅

**Required fix:** convert to `sqlx::query!` macros; `cargo sqlx prepare`; commit `.sqlx` cache.

**Evidence:**

1. **All 6 new DAOs converted to compile-time macros** (`crates/nexus-local-db/src/works.rs` + `crates/nexus-local-db/src/work_chapters.rs`):
   - `works::set_auto_chronology`: `sqlx::query!` (UPDATE, 3 binds).
   - `works::get_auto_chronology`: `sqlx::query_scalar!` (`SELECT auto_chronology as "auto_chronology!" FROM works WHERE work_id = ?`).
   - `works::list_works_with_auto_chronology`: `sqlx::query_as!` (WorkAutoChronologyRow; `work_id!`, `creator_id!`, `intake_status!`, `title!`, `total_planned_chapters: i32`, nullable `work_ref` / `runtime_lock_holder` / `completion_locked_at` correctly left non-annotated).
   - `work_chapters::current_volume`: `sqlx::query_scalar!` (`SELECT MAX(volume) as "volume: i32" FROM work_chapters WHERE work_id = ?`).
   - `work_chapters::is_volume_fully_finalized`: `sqlx::query!` (`COUNT(*) as "total_rows!"`, `COALESCE(SUM(...), 0) as "finalized_rows!"`).
   - `work_chapters::seed_volume_chapters_tx`: `sqlx::query!` (INSERT OR IGNORE inside tx; 8 binds).
2. **`.sqlx/query-*.json` cache populated** — 6 new query metadata files committed (matching the 6 macros); `.sqlx/state.db` is gitignored.
3. **`SQLX_OFFLINE=true` build works** (the macros expand at compile time and use the offline cache):
   - `SQLX_OFFLINE=true cargo test -p nexus-orchestration --test auto_chronology_tick` → 12 passed.
   - `SQLX_OFFLINE=true cargo test -p nexus-daemon-runtime --test auto_chronology_task` → 3 passed.
   - `SQLX_OFFLINE=true cargo test -p nexus-local-db --lib` → **227 passed, 0 failed**.
   - `cargo build --workspace` → clean (Finished `dev` profile in 18.57s, no errors).
4. **Type annotations** — SQLite `INTEGER` → `i64` overrides (`total_planned_chapters: i32`, `volume: i32`) match the `Option<i32>` / `Option<i32>` return types on the DAOs; `field!` annotations are correctly applied to all NOT NULL columns.

**Scope note from commit 42ce8bfa**: the implementer explicitly carved out the `nexus-orchestration`-side runtime queries (`perform_advance`'s `UPDATE works SET updated_at`, `load_row_for_manual`'s SELECT) as out of scope. Those queries live in `nexus-orchestration`, which is not bound by `nexus-local-db/AGENTS.md`'s compile-time macro rule (the W-3 fix targeted the 6 new DAOs explicitly listed). Pre-existing technical debt of the same class in `nexus-orchestration` is properly deferred to avoid piggyback scope creep. **No follow-up needed** for V1.50 T-A P3; a residual entry or future V1.50 P-last sweep may revisit.

### CI Gates (re-run)

| Gate | Command | Result |
|------|---------|--------|
| Orchestration tests | `cargo test -p nexus-orchestration --test auto_chronology_tick` | **12 passed, 0 failed** (12 ✓ required) |
| Daemon tests | `cargo test -p nexus-daemon-runtime --test auto_chronology_task` | **3 passed, 0 failed** (3 ✓ required) |
| Local-DB tests | `SQLX_OFFLINE=true cargo test -p nexus-local-db --lib` | **227 passed, 0 failed** |
| Workspace build | `cargo build --workspace` | **clean** (Finished `dev` profile in 18.57s) |
| SQLX offline build | `SQLX_OFFLINE=true cargo test` (per affected target) | **clean** (proves the new `sqlx::query!` macros compile and the `.sqlx` cache is correct) |
| Clippy | `cargo clippy --all -- -D warnings` | **clean** (Finished `dev` profile; no warnings or errors) |
| Nightly fmt | `cargo +nightly fmt --all --check` | **exit 0** |
| CLI integration | `cargo test -p nexus42 --test chronology_cli` | **10 passed, 0 failed** (incl. `chronology_advance_help_documents_force_flag`) |
| CLI help sanity | `cargo run --bin nexus42 -- creator works chronology advance --help` | shows `--force` with correct doc string |

### New Findings (this re-review wave)

None. The 3 blocking Warnings are resolved with structural fixes + regression tests + CI-clean evidence. The 4 Suggestion items from the initial wave (S-1 index, S-2 spec wording, S-3 clippy `--all-targets`, S-4 temp-file suffix) remain non-blocking and are not in scope for this targeted re-review per Assignment.

### Residual Items (unchanged from initial wave)

- S-1 (index on `works.auto_chronology`): non-blocking; recommend V1.50 P-last hygiene sweep.
- S-2 (CompletionLocked log message conflation): non-blocking; spec/code alignment tracked in completion report's R-V150P3AUTOCHRONO-01.
- S-3 (clippy `--all-targets` doc lint L75 + wildcard L330 in `auto_chronology_tick.rs`): non-blocking; the CI gate is `cargo clippy --all -- -D warnings` (lib + bin only), which is clean. Stricter `--all-targets` would surface these. Auto-fixable; recommend before merge or P-last hygiene.
- S-4 (atomic write temp-file suffix via `.with_extension("md.tmp")`): non-blocking; functionally safe due to per-Work-id serialization at the gate.

### Summary (Revalidation)

| Severity | Count (this wave) | Status |
|----------|-------------------|--------|
| 🔴 Critical | 0 | — |
| 🟡 Warning | 0 (all 3 resolved) | W-1 ✅, W-2 ✅, W-3 ✅ |
| 🟢 Suggestion | 0 new | S-1..S-4 unchanged (non-blocking) |

**Verdict (Revalidation):** **Approve**

Rationale: all 3 blocking Warning findings from the initial wave are resolved with structural fixes that match the original required-fix descriptions:
- W-1: DB tx now commits before outline write (Design A "DB first, FS second"); `--force` CLI flag added for recovery; 2 regression tests cover both branches.
- W-2: pure `parse_interval_secs(Option<&str>)` extracted; tests pass values directly without mutating process-global env — eliminates the V1.49 R-V149P1-02 flake pattern entirely.
- W-3: all 6 new DAOs converted to `sqlx::query!` / `sqlx::query_as!` / `sqlx::query_scalar!` compile-time macros; `.sqlx` cache committed; `SQLX_OFFLINE=true` build proves compile-time correctness.

CI gates all green (clippy, nightly fmt, workspace build, targeted tests, CLI integration). The CLI `--force` flag is wired through the CLI → orchestration → DAO chain end-to-end. The implementation is structurally sound for V1.50 T-A P3 ship.

Recommend: PM consolidate, archive the 3 R-V150P3AUTOCHRONO-0[4,5,6] residual_findings to `archived/residuals/`, advance `2026-06-18-v1.50-auto-chronology` to `Done` (after QA verifies).

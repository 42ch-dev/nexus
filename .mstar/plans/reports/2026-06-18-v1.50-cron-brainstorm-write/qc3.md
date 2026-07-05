---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-18-v1.50-cron-brainstorm-write
verdict: Approve with residuals
generated_at: 2026-06-17T11:30:00Z
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: MiniMax-M3
- Review Perspective: Performance + reliability (focus on hot-path CPU cost, tracing-log volume, partial-index coverage, idempotency under concurrency, daemon task lifecycle, hermetic test fidelity vs. V1.49 tracing-registry flake, preset-version drift surface)
- Report Timestamp: 2026-06-17T11:30:00Z

## Scope
- plan_id: 2026-06-18-v1.50-cron-brainstorm-write
- Review range / Diff basis: merge-base 0ea2995ff45569b541b17097c4c919dabab4bb16..f16daaddf616583e1ee85f2a9cfa8c6db7f15b18
- Working branch (verified): feature/v1.50-cron-brainstorm-write
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-brainstorm-write
- Files reviewed: 16 (plan + status + 14 source files incl. 3 new test modules)
- Commit range: 0ea2995ff45569b541b17097c4c919dabab4bb16..f16daaddf616583e1ee85f2a9cfa8c6db7f15b18
- Tools run: git diff, git show, grep, read, cargo check, cargo clippy (touched crates, -D warnings), cargo +nightly fmt --check (touched crates), cargo test -p nexus-orchestration --test cron_supervisor (18 passed), cargo test -p nexus-daemon-runtime --test cron_supervisor_task (2 passed), cargo test -p nexus-orchestration --lib (649 passed, 1 ignored; no regression), cargo test -p nexus42 --lib cron (29 passed)

## Findings

### 🔴 Critical
None.

### 🟡 Warning

- **W-001 (hot-path CPU): `cron_fires_at_minute` reparses the cron expression on every tick.** At `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:330`, `cron::Schedule::from_str(&normalized)` is called per (Work × role) per tick. `cron::Schedule` is a non-trivial struct (Cuckoo filters + bitmaps); parsing a 5/6-field expression costs on the order of 10–50 µs on a modern CPU. For 1 000 Works × 3 roles per minute, this is ~30 ms of CPU per tick dedicated solely to re-parsing — the cron expression for a given (Work, role) is stable until the user re-runs `creator works cron set`. At 100 ms sustained, this is a measurable slice of the 60-second budget; the partial index already keeps the scan cheap, but the parse-loop eats most of that savings back. Recommend (a) parse once on `set_schedule_json` write and store a normalized form in `works.schedule_json` itself, OR (b) memoise per-Work using a small in-process `moka`/`HashMap<(work_id, role), Schedule>` cache keyed on the raw cron string so invalidation is trivial (different input → reparse). Either fix turns the per-tick cost O(1) per Work after the first observation. **Severity rationale**: not Critical because the path is bounded by SQLite single-writer serialisation and the daemon's 60-second tick budget has slack; flagged Warning because the cost grows linearly with cron-using Works and there is no current cap on `list_works_with_schedule_json` returning rows.

- **W-002 (tracing volume): the idempotency-skip log line is `info!`-level and will become noisy at scale.** At `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:255`, `info!(... "cron-supervisor: prior schedule still active; skipping fire")` fires on every active prior schedule. A normal workday cadence for a single author with three Works on a 4×/day brainstorm + 4×/day write config = ~24 fires/day ≈ 1 skip/minute under normal operation. A workspace with 100 Works all on `* * * * *` for brainstorm + write = ~200 skips/minute → ~12 000 info lines/hour. The `sweep complete` summary line at line 146 already aggregates the counters, so the per-skip `info!` is redundant for observability and should be dropped to `debug!`. **Severity rationale**: not Critical (no functional defect), Warning because log-volume growth is an operational reliability concern and the threshold for action (file-rotation rate, journald caps, disk pressure on `~/.nexus42/`) is reached well before 1 000 active Works.

- **W-003 (preset-version drift surface): `novel-brainstorm` / `novel-write` fall through to `_ => 1` in `preset_version_for_id` and there is no test guard.** At `crates/nexus-orchestration/src/auto_chain.rs:1451`, `novel-brainstorm` and `novel-write` are not in the explicit match arms; both default to version 1. The `preset_version_mapping_matches_yaml` sync test (line 2127) only iterates over `["novel-writing", "research", "novel-review-master", "kb-extract"]`. When the deferred `novel-write` preset (R-V150P1CRONBW-01) is authored in T-A P2, if its `version:` field is bumped above 1, the cron evaluator will silently stamp `preset_version = 1` on every cron-fired schedule, and the loader will treat those schedules as a stale version per `auto_chain.rs:2127` comment. The `novel-brainstorm` preset has the same risk — its embedded YAML is shipped (verified in plan artifact table), but its version is not asserted by the sync test. **Severity rationale**: not Critical (no immediate functional regression — current schedule-row versioning tolerates a downgrade with a warn), Warning because the failure mode is silent and the cost to fix is one match arm + two assertions.

### 🟢 Suggestion

- **S-001 (per-tick work allocation):** `evaluate_cron_fires` collects all `WorkCronRow`s into a `Vec` (line 129) before iterating. For 1000 Works × ~250 bytes/row that is ~250 KiB on the heap per tick (60×/hour). A `fetch` (sqlx `try_stream`) iterating row-by-row would cap memory at one row at the cost of slightly more complex borrow management. Suggested only if profiling later shows the heap allocation is meaningful; current sizing is fine.

- **S-002 (per-Work TZ conversion redundancy):** At line 334, `tz.from_utc_datetime(&now.naive_utc())` is called once per Work inside `cron_fires_at_minute`. Since `now` is the same across all Works in a tick, the local-time truncation for the current minute could be hoisted into `evaluate_cron_fires` and passed down. Saves 1 000 TZ conversions per tick at 1 000 Works. Negligible per call (~µs), but trivially avoidable.

- **S-003 (per-Work JSON parse):** `serde_json::from_str(&row.schedule_json)` (line 165) happens once per Work per tick. Same memoisation argument as W-001 — a parsed `CronConfig` could be cached by `schedule_json` bytes (or by content-hash) for the lifetime of a tick. Optional; not in the hot path until Works cross ~10⁴.

- **S-004 (cron supervisor shutdown drain):** `spawn_cron_supervisor` (line 87) uses `tokio::select!` over `ticker.tick()` and `shutdown_notify.notified()`. If `shutdown_notify` fires mid-`run_one_tick`, the in-progress tick completes before the next iteration of the loop checks shutdown. For the worst-case 1 000-Work tick (tens of ms), this delay is benign, but the daemon's graceful-shutdown sequence (`boot.rs:520`) awaits `lifecycle.wait_until_terminal()`, so a hung cron task would extend shutdown by a full tick. Adding `tokio::select!` around `run_one_tick` itself — or using `JoinHandle::abort()` on the shutdown branch — would make the lifecycle deterministic. Note this is a future-proofing improvement, not a current bug.

- **S-005 (lost fires on restart):** No catch-up logic exists for cron minutes that passed while the daemon was stopped (e.g., laptop asleep during the 03:00 cron). The `cron::Schedule::after(&just_before)` pattern in `cron_fires_at_minute` (line 349) only matches the current minute, not missed minutes. This is acceptable for typical cron semantics (cron historically only fires at the moment, not catch-up), but the README and `nexus-home-layout`/`nexus-daemon-runtime` docs do not call this out. A one-line note in the user-facing `creator works cron show` help text + the spec doc would close the surprise gap. Tracked under a fresh residual candidate if PM wants to formalise.

- **S-006 (idempotency-COUNT vs. existence):** `has_active_role_schedule` (line 377) runs `SELECT COUNT(*) FROM creator_schedules WHERE work_id = ? AND preset_id = ? AND status IN (...)` and converts to `bool`. SQLite is smart enough to use the same index for COUNT as for an existence check, but a `LIMIT 1` would make the intent explicit and shave a small constant. Cosmetic; same plan.

- **S-007 (cron daemon task: explain why supervisor tick is gated on `fired > 0`):** At line 111, `run_one_tick` only calls `supervisor.tick_clocked` when `summary.fired > 0`. The inline comment justifies this correctly. If a future maintainer adds another enqueue path that goes through this task, the implicit "no fires → no admission scan" assumption could regress silently. A one-line `// assumption:` comment naming the invariant would harden it.

## Source Trace
- Finding ID: W-001
- Source Type: manual-reasoning + code review (`crates/nexus-orchestration/src/schedule/cron_supervisor.rs:328-350`)
- Source Reference: `cron::Schedule::from_str(&normalized)` re-invoked per `(work, role)` per tick
- Confidence: Medium

- Finding ID: W-002
- Source Type: code review (`crates/nexus-orchestration/src/schedule/cron_supervisor.rs:253-261`)
- Source Reference: `info!` macro on idempotency skip path; per-Works scaling analysis
- Confidence: High

- Finding ID: W-003
- Source Type: code review (`crates/nexus-orchestration/src/auto_chain.rs:1451-1468`, `:2127-2174`)
- Source Reference: `_ => 1` fallback covers both new cron preset ids; sync test array omits them
- Confidence: High

- Finding IDs: S-001 .. S-007
- Source Type: code review + manual-reasoning
- Source Reference: inline as listed above
- Confidence: Medium

## Detailed Performance + Reliability Analysis (per assignment focus)

### 1. 1-min tick scan — S-001 partial index actually covers the query?
**Verified.** `crates/nexus-local-db/migrations/202606180002_works_schedule_json_partial_idx.sql` creates `idx_works_schedule_json_nonempty ON works (schedule_json) WHERE schedule_json IS NOT NULL AND schedule_json != ''`. The DAO `list_works_with_schedule_json` (`crates/nexus-local-db/src/works.rs:1498-1513`) issues `WHERE schedule_json IS NOT NULL AND schedule_json != ''`, matching the partial-index predicate. The hermetic test `partial_index_used_in_schedule_json_scan` (`crates/nexus-orchestration/tests/cron_supervisor.rs:538-562`) asserts via `EXPLAIN QUERY PLAN` that the plan text contains `idx_works_schedule_json_nonempty`. Re-ran locally — **passes**.

### 2. Cron evaluator CPU cost at scale: 1000 Works × 3 roles = 3000 cron evaluations per minute?
**Bounded but hot-path-heavy.** See W-001. At 3000 evaluations/min, total work per tick is dominated by `cron::Schedule::from_str` (~30 ms combined), the per-Work JSON parse (~10 ms combined), and the per-Work TZ conversion (~10 ms combined). None of these are individually expensive; the issue is that all three are O(N) per tick and avoidable. Suggestion S-002 / S-003 partially address; W-001 is the structural fix. The single-tick 60-second budget absorbs this comfortably today, but at ~10⁴ Works the parse-loop alone starts to dominate.

### 3. `evaluate_cron_fires` per-tick cost — bounded? Does it short-circuit on a non-match minute?
**Partially bounded.** On a non-match minute, the loop still:
1. Reads all rows from the partial index (`list_works_with_schedule_json`)
2. Parses each `schedule_json` blob (line 165)
3. Validates each TZ (line 179)
4. Iterates both roles per Work
5. Skips at `cron_fires_at_minute` returning false (line 230)
The role-level short-circuit is correct: `cron_fires_at_minute` is the first expensive call in the role path, and it returns `false` quickly for non-matches. The Work-level parsing (JSON + TZ) is **not** short-circuited because parsing happens once per Work in `evaluate_work` (line 159). This is the right structure (parse once, evaluate both roles), so the recommendation is parse-once-and-cache rather than reorder. No correctness issue.

### 4. `set_schedule_json_tx` retry semantics on contention — exponential backoff or simple retry?
**No automatic retry; CLI surfaces a clear user-facing message.** The CAS writer at `crates/nexus-local-db/src/works.rs:1538-1584` returns `Ok(false)` on mismatch and `Err(MissingVersionKey)` on missing row. The CLI caller at `crates/nexus42/src/commands/creator/works/cron.rs:647-675` rolls back the transaction and returns `CliError::Config("schedule_json changed by another writer between read and write; re-run `creator works cron set` to re-apply against the latest config")`. The daemon cron evaluator never writes `schedule_json` (read-only), so the racing party is only concurrent CLI invocations or future admin paths. No exponential backoff is implemented or needed — the CLI's user-driven retry is the right granularity. qc2 already verified this path; concurring.

### 5. Daemon task shutdown — graceful drain? Lost fires on restart documented?
**Shutdown is best-effort, not draining.** `spawn_cron_supervisor` (`crates/nexus-daemon-runtime/src/cron_supervisor.rs:70-99`) uses `tokio::select!` to listen for `shutdown_notify.notified()`. The current tick completes before the loop re-checks shutdown (Suggestion S-004). No explicit `JoinHandle::abort()` on shutdown. For the daemon's overall shutdown sequence (`boot.rs:520-535`) which calls `lifecycle.wait_until_terminal()` after the supervisor pause + engine cancel, an in-flight cron tick only delays the terminal state by ≤1 tick (~ms), not seconds. **No drain needed for correctness** because cron-enqueued schedules are out-of-band and persisted; the supervisor's `resume_running_as_paused("daemon_restart")` recovers pending schedules on the next boot.

**Lost fires on restart: not caught up.** See Suggestion S-005. The spec implicitly accepts cron-style "fire at the moment, not catch up," and the bootstrap does not replay missed minutes. Worth a docs note rather than a code change.

### 6. Tracing log volume — 1-min tick with 1000 Works could emit 3000+ debug logs/min — is this rate-limited?
**Mostly debug-level, but one hot info! slips through.** Per-tick events:
- `sweep complete` summary line (`info!`, line 146) — 1/minute. Fine.
- Per-role enqueue (`info!`, line 273) — fires only when `fired > 0`. Bounded by `enqueue_cron_schedule` rate. Fine.
- Per-role idempotency skip (`info!`, line 255) — **fires whenever a prior same-role schedule is active** (i.e., nearly always during the active life of a recently-fired role). This is the noisy line flagged in W-002.
- Per-role disabled skip (`debug!`, line 223) — silent at default log level. Fine.
- Per-role gated skip (`debug!`, line 238) — silent at default log level. Fine.
- Per-Work parse error (`warn!`, line 168/181) — bounded by bad-config count. Fine.

The `info!`-on-skip is the only volume concern. Downgrading to `debug!` is a one-line change and aligns with the rest of the skip paths.

### 7. `R-V150P1CRONBW-01` (novel-write preset not yet authored) — correctness regression or separate workstream?
**Separate workstream; cron evaluator contract is intact.** Confirmed:
- `evaluate_cron_fires` enqueues `preset_id = "novel-write"` correctly (`crates/nexus-orchestration/src/schedule/cron_supervisor.rs:262-269`).
- `enqueue_cron_schedule` persists the row with the correct preset id (`crates/nexus-orchestration/src/auto_chain.rs:1579-1599`).
- The `enqueue_cron_schedule` label is `format!("cron:{role}:{work_id}")`, which surfaces the role correctly.
- The supervisor `tick_clocked` admits it like any other pending schedule.
- The failure mode is at the **executor** layer when it tries to resolve the preset id; this is out of scope for the cron evaluator. The plan already declares this residual `decision: defer, target: V1.50 T-A P2 or dedicated preset-authoring plan`. The contract is intact.

### 8. Hermetic test for cron_supervisor — 18 tests, susceptible to V1.49's R-V149P1-02 tracing-registry flake pattern?
**No flake surface.** Reviewed all 18 tests in `crates/nexus-orchestration/tests/cron_supervisor.rs` and the 2 tests in `crates/nexus-daemon-runtime/tests/cron_supervisor_task.rs`. None install a `tracing_subscriber::fmt().init()` (the source of the V1.49 pattern), none make log-level assertions, and each test creates its own SQLite database via `tempfile::Builder::new().tempfile().unwrap()` then `std::mem::forget(db)`. The `partial_index_used_in_schedule_json_scan` test asserts on `EXPLAIN QUERY PLAN` output text — SQLite's planner is deterministic on a fixed schema, so this is stable. **Susceptibility: none.**

Additional reliability note: the `cron_supervisor_task` tests use `tokio::test` with a per-test fresh pool, which is the right hermetic discipline. The 18 hermetic tests re-ran clean locally (18 passed, 0 failed, 1.35s) and in release (0.72s).

## Residual Registration Hints (for PM consolidated decision)

The plan already registers:
- `R-V150P0-W5` — **resolved** in `67db009b` (CAS writer, qc2 confirmed). ✓
- `R-V150P1CRONBW-01` — `novel-write` preset authoring gap, `severity: medium`, `decision: defer`, `target: V1.50 T-A P2 or preset-authoring plan`. ✓ Appropriate.
- `R-V150P1CRONBW-02` — `cargo clippy --all -- -D warnings` not run locally, `severity: low`, `decision: accept`. ✓ Appropriate.

**Suggested new residual** (for PM to triage — not blocking):
- `R-V150P1CRONBW-03` — `cron_fires_at_minute` re-parses the cron expression per (Work, role) per tick (W-001). Severity: medium. Decision: defer (cache invalidation correctness needs design — likely belongs in V1.50 P1+ or a cron-cache plan). Owner: `@fullstack-dev`.
- `R-V150P1CRONBW-04` — idempotency-skip tracing line is `info!`-level, noisy at scale (W-002). Severity: low. Decision: accept (one-line fix is trivial; if PM wants to formalise, mark as `defer` for a logging-policy pass).
- `R-V150P1CRONBW-05` — `novel-brainstorm` / `novel-write` not in `preset_version_for_id` match arms; sync test omits them (W-003). Severity: low. Decision: defer (roll into the same T-A P2 preset-authoring plan as R-01; adds 3 lines).

These are listed for PM's consolidated-decision workflow; I am NOT writing them to `status.json` per QC's role boundary.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 7 |

**Verdict**: Approve with residuals

**Rationale**:
- No Critical findings.
- The three Warning findings are real but **not blocking**: all three are optimisations or hardening for future scale (W-001) and quiet ops (W-002, W-003). None affect the correctness of cron firing, idempotency, gating, or the daemon's 1-min tick contract.
- All 18 hermetic tests + 2 daemon integration tests pass. Clippy is clean on the four touched crates. Nightly fmt is clean. No regressions in `nexus-orchestration --lib` (649 passed) or `nexus42 --lib cron` (29 passed).
- The cron evaluator contract is intact and complete; the deferred `novel-write` preset (R-V150P1CRONBW-01) is a separate workstream that does not affect this plan's AC.
- The partial-index coverage is verified by an `EXPLAIN QUERY PLAN` assertion test; the CAS writer's TOCTOU closure is verified by four logical-race tests.

PM may proceed to consolidated decision. The three Warning items are appropriate to register as open residuals (R-V150P1CRONBW-03 / -04 / -05) so they don't get lost, but they do not block the merge of this plan.

---

## Completion Report v2

**Agent**: qc-specialist-3
**Task**: V1.50 T-A P1 cron-brainstorm-write — QC tri-review #3 (performance + reliability)
**Status**: Done
**Scope Delivered**: Initial-wave QC review with focus on performance hot path, tracing log volume, partial-index coverage, idempotency semantics, daemon task lifecycle, hermetic test fidelity, and preset-version drift surface. Reviewed 16 files across 4 commits in the assigned `0ea2995ff…f16daadd` range. Verified tests pass locally, clippy clean, nightly fmt clean on touched scope.
**Artifacts**: `.mstar/plans/reports/2026-06-18-v1.50-cron-brainstorm-write/qc3.md` (this file)
**Validation**:
- `cargo check -p nexus-orchestration -p nexus-local-db -p nexus42 -p nexus-daemon-runtime` → exit 0
- `cargo clippy -p nexus-orchestration -p nexus-local-db -p nexus42 -p nexus-daemon-runtime -- -D warnings` → exit 0
- `cargo +nightly fmt -p nexus-orchestration -p nexus-local-db -p nexus42 -p nexus-daemon-runtime -- --check` → exit 0
- `cargo test -p nexus-orchestration --test cron_supervisor` → 18 passed, 0 failed
- `cargo test -p nexus-daemon-runtime --test cron_supervisor_task` → 2 passed, 0 failed
- `cargo test -p nexus-orchestration --lib` → 649 passed, 1 ignored; no regression
- `cargo test -p nexus42 --lib cron` → 29 passed, 0 failed; CAS path verified

**Issues/Risks**: 3 Warning findings (cron re-parse hot path, idempotency-skip info! volume, preset-version drift surface) — none blocking; recommended as new open residuals for PM consolidated decision. 7 Suggestions for future hardening.

**Plan Update**: None required from QC. PM owns `status.json` residual lifecycle per `mstar-plan-artifacts`. Suggested new residual IDs `R-V150P1CRONBW-03 / -04 / -05` listed in the report for PM to register if accepted.

**Handoff**: PM to consolidate with qc1 (architecture / maintainability) and qc2 (security / correctness, already filed and `Approve`). My verdict is `Approve with residuals` — no blocking issues. Plan is ready for PM consolidated decision and merge to `iteration/v1.50` per plan §5.

**Git**: `f04ecef0` — `qc(v1.50): T-A P1 cron-brainstorm-write QC review #3 (perf + reliability, Approve w/ residuals)` (commit on `feature/v1.50-cron-brainstorm-write`, single file staged: `.mstar/plans/reports/2026-06-18-v1.50-cron-brainstorm-write/qc3.md`, 184 insertions; hash final after post-commit back-fill amend).

---

## Revalidation

```yaml
report_kind: qc-revalidation
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-18-v1.50-cron-brainstorm-write
working_branch: feature/v1.50-cron-brainstorm-write
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-cron-brainstorm-write
review_range: f16daadd..f60270a4
fix_wave_commits:
  - 4c305674 (R-V150P1CRONBW-03 memoise cron parse)
  - 75515b27 (R-V150P1CRONBW-04 debug! downgrade)
  - abfbb855 (R-V150P1CRONBW-05 explicit match arms)
  - f60270a4 (plan completion report)
verdict: Approve
generated_at: 2026-06-17T12:00:08Z
```

### Context

Targeted re-review of the 3 blocking Warning findings raised in the initial wave (R-V150P1CRONBW-03 / -04 / -05). The original `## Scope` and `## Findings` sections above remain immutable. The fix-wave covers `f16daadd..f60270a4` (4 commits: 3 fix commits + 1 docs commit). qc1 (architecture / maintainability) and qc2 (security / correctness) were `Approve` in the initial wave; this re-review is performed by qc-specialist-3 only.

**Tools run (re-review):**
- `git diff f16daadd..f60270a4 --stat` → 7 files, +934 / −11
- `git show 4c305674` / `75515b27` / `abfbb855` for per-commit evidence
- `grep` + targeted `read` for the `info!` → `debug!` downgrade
- `cargo test -p nexus-orchestration --lib schedule::cron_supervisor::tests::cron_fires_at_minute_uses_memoised_schedule` → **1 passed, 0 failed** (R-03 regression test)
- `cargo test -p nexus-orchestration --lib auto_chain::tests::preset_version` → **2 passed, 0 failed** (R-05 regression tests)
- `cargo test -p nexus-orchestration --lib schedule::cron_supervisor` → **14 passed, 0 failed** (full module; was 13, +1 new)
- `cargo test -p nexus-orchestration --test cron_supervisor` → **18 passed, 0 failed** (no regression)
- `cargo test -p nexus-daemon-runtime --test cron_supervisor_task` → **2 passed, 0 failed** (no regression)
- `cargo clippy -p nexus-orchestration -p nexus-local-db -p nexus42 -p nexus-daemon-runtime -- -D warnings` → **clean**
- `cargo +nightly fmt -p nexus-orchestration -p nexus-local-db -p nexus42 -p nexus-daemon-runtime -- --check` → **clean**

### Per-W Disposition

**R-03 (W-001 hot-path CPU, R-V150P1CRONBW-03): Resolved — evidence `4c305674` + passing regression test.**

- `cron_fires_at_minute` (the per-Work raw-string parser) is now `#[cfg(test)]` and retained as a matcher-semantics fixture only (`crates/nexus-orchestration/src/schedule/cron_supervisor.rs:385-393`).
- A new wrapper `cron_fires_at_minute_for_work` (lines 425-465) owns the daemon hot path. It looks up a `OnceLock<Mutex<HashMap<(String, String), (String, cron::Schedule)>>>` keyed by `(work_id, role)`, stores the raw cron string alongside the parsed `Schedule`, and re-parses only on cache miss **or** raw-string drift (content-based invalidation).
- The critical section is tight: the `Schedule` is cloned out of the mutex before the µs-scale `schedule_fires_at_minute` matcher runs.
- `try_fire_role` now calls the cached wrapper (line 233).
- A public `invalidate_cron_schedule_cache` (lines 367-373) is exposed from `nexus_orchestration::schedule::cron_supervisor` and invoked by `nexus42::commands::creator::works::cron::handle_set` (line 680) after a successful `set_schedule_json_tx` CAS write. The daemon is read-only on `schedule_json`, so the CLI write site is the only invalidation hook (correctly documented in the fix commit message).
- Regression test `cron_fires_at_minute_uses_memoised_schedule` (lines 624-722) uses an instrumented `AtomicU64` parse counter to assert: 100 calls for the same `(work, role, cron)` parse exactly once, repeat calls stay at 1, content drift on the same key re-parses (counter → 2), a new `(work, role)` re-parses (counter → 3), non-match minute does not re-parse (counter stays at 3), and `invalidate_cron_schedule_cache` forces a re-parse (counter → 4). **All 4 assertions pass locally.**
- One minor implementation note: the cache is process-global, which is correct for the single-daemon-singleton topology documented in the plan §3. No thread-safety regression (Mutex is std, OnceLock initialisation is one-shot). The clippy-clean build confirms no `unwrap` on poisoned mutex propagation issues.

**R-04 (W-002 tracing volume, R-V150P1CRONBW-04): Resolved — evidence `75515b27` + manual source verification.**

- One-line `info!` → `debug!` change at `crates/nexus-orchestration/src/schedule/cron_supervisor.rs:263` (idempotency-skip branch in `try_fire_role`).
- A 5-line justification comment (lines 258-262) names the residual id, the qc3 finding, the throughput reasoning (~12k info lines/hour at 100 Works on a 4×/day cadence), and the design rationale (sweep-complete summary line already aggregates the counter). The comment is sufficient for any future maintainer who scans the hot path.
- The two other skip paths (disabled, gated) were already `debug!`, so this change brings the idempotency path into alignment — net effect: default-level logs now show only the `sweep complete` summary once per minute plus per-fire enqueues, with all three skip categories silent at the default level.
- No regression test was added, which is consistent with the assignment's "no automated test" guidance and the commit's own note: the `sweep complete` summary line is the observable counter for ops. Log-level verification is behavioural and the field-level clippy check (`debug!` exists with the same `work_id` / `role` / `preset_id` fields) is sufficient to prevent accidental re-bumps.

**R-05 (W-003 preset-version drift surface, R-V150P1CRONBW-05): Resolved — evidence `abfbb855` + 2 passing regression tests.**

- An explicit match arm `"novel-brainstorm" | "novel-write" => 1` was added to `preset_version_for_id` (`crates/nexus-orchestration/src/auto_chain.rs:1477`). Both cron-triggered preset ids are now visible in the version map (discoverability for a future bump).
- The `#[allow(clippy::match_same_arms)]` is documented in a 7-line comment (lines 1451-1459) explaining that the named arms intentionally share the catch-all body — the explicit arms exist for *discoverability* (a maintainer scanning the map sees the cron-triggered presets and knows to bump them in lockstep with their `preset.yaml`), and the regression test enforces the sync, so the named arms are documentation, not a behavioural fork. This is the correct trade-off given the version-policy invariant (R-V139P5-W-4).
- The sync test was renamed to `preset_version_mapping_matches_yaml_includes_cron_presets` (lines 2146-2157) and its `known_ids` array extended with `"novel-brainstorm"` and `"novel-write"`. The test now strictly validates `novel-brainstorm` against its embedded `preset.yaml` version, and tolerates the deferred `novel-write` (asserts `mapping == 1` + cites `R-V150P1CRONBW-01` until authored). The strict-YAML-sync branch then auto-enforces the comparison the moment the preset YAML lands.
- A focused regression test `preset_version_for_id_novel_brainstorm_resolves` (lines 2217-2250) was added — it explicitly guards `novel-brainstorm`'s mapping against silent drift even if someone later prunes the `known_ids` array in the sync test above. This is the right defensive test placement.
- Both tests pass locally: `preset_version_for_id_novel_brainstorm_resolves` and `preset_version_mapping_matches_yaml_includes_cron_presets`. The full `nexus-orchestration --lib` now reports 651 passed (was 649, +2 new), 0 failed, 1 ignored — matches the fix commit's own verification line.

### New Findings (specific to fix-wave)

None. The fix-wave is surgical and contained:

- **R-03:** 2 files touched, +228 / −2; the public invalidation API is correctly scoped and called from exactly one site (the CAS write path).
- **R-04:** 1 file, +6 / −1; pure logging-level downgrade.
- **R-05:** 1 file, +85 / −7; 2 new tests, 1 test renamed + extended.

No new clippy warnings, no fmt drift, no public API surface change beyond `invalidate_cron_schedule_cache` (a clean `pub fn` with no safety contract surprises). The pre-existing `Suggestion` items S-001..S-007 from the initial wave remain open as forward-looking hardening; they are out of scope for the re-review and not blocking.

### Residual Disposition

The 3 residuals recommended by qc3 in the initial wave are now **resolved in-tree** (not "deferred") because the fix-wave delivered each one. PM-owned `status.json` lifecycle (per `mstar-plan-artifacts`): QC does not modify `status.json` directly. Recommendation to PM for residual closure:

- `R-V150P1CRONBW-03` → mark `lifecycle: resolved` with `resolution.commit = 4c305674` and `resolution.evidence_test = "schedule::cron_supervisor::tests::cron_fires_at_minute_uses_memoised_schedule"`.
- `R-V150P1CRONBW-04` → mark `lifecycle: resolved` with `resolution.commit = 75515b27` and `resolution.evidence_test = "(behavioural: per-skip `debug!` at cron_supervisor.rs:263)"`.
- `R-V150P1CRONBW-05` → mark `lifecycle: resolved` with `resolution.commit = abfbb855` and `resolution.evidence_test = "auto_chain::tests::preset_version::{preset_version_for_id_novel_brainstorm_resolves, preset_version_mapping_matches_yaml_includes_cron_presets}"`.

The pre-existing open residual `R-V150P1CRONBW-01` (`novel-write` preset authoring gap) is **out of scope** for this re-review and remains `defer` per the plan §5 / T-A P2.

### Verdict

**Verdict**: **Approve**

**Rationale:**
- All 3 blocking Warning findings (R-03 / R-04 / R-05) are properly resolved with surgical, well-commented, test-covered fixes.
- The new regression tests directly assert the invariants the findings were protecting (memoised parse count = 1 for 100 calls; preset-version mapping in sync with embedded YAML for cron-triggered presets).
- All 14 schedule::cron_supervisor lib tests, 18 cron_supervisor integration tests, 2 cron_supervisor_task daemon tests, and 2 preset_version tests pass locally with zero regressions.
- Clippy is clean on the four touched crates; nightly fmt is clean.
- No new findings introduced by the fix-wave.
- The plan is ready for PM consolidated decision and merge to `iteration/v1.50` per plan §5. The 3 Warning items are no longer "Approve with residuals" — they are fully closed in this fix-wave.
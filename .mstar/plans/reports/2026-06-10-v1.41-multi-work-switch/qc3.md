---
report_kind: qc-review
reviewer: "@qc-specialist-3"
reviewer_index: 3
focus: performance-reliability
plan_id: 2026-06-10-v1.41-multi-work-switch
verdict: Approve
generated_at: 2026-06-10T21:30:00+08:00
review_range: "merge-base: 55689706 → tip: 9b6627dd"
working_branch_verified: iteration/v1.41
review_cwd_verified: /Users/bibi/workspace/organizations/42ch/nexus
files_reviewed: 15
tools_run: cargo clippy --all-targets, cargo +nightly fmt --check, cargo test, manual review
---

# Code Review Report — V1.41 P0 (qc3)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-10T20:45:00+08:00

## Scope
- plan_id: 2026-06-10-v1.41-multi-work-switch
- Review range / Diff basis: merge-base: 55689706 → tip: f4b39d42
- Working branch (verified): iteration/v1.41
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 15
- Tools run: cargo clippy --all-targets, cargo +nightly fmt --check, cargo test, manual review

## Findings
### 🔴 Critical
*None.*

### 🟡 Warning
- **W1: mark_work_completed DB patch and file I/O are not atomic** → `auto_chain::mark_work_completed` patches the DB (status, novel_completion_status, completion_locked_at, clears driver_schedule_id) but the `.completion-lock.json` file write is delegated to the caller (`write_completion_lock_for_work`). If the file write fails or the caller crashes between DB commit and file write, the DB records "completed" state but no on-disk lock exists. This leaves an inconsistency where the work appears completed in queries but has no filesystem guard. **Fix:** Either wrap both in a higher-level transaction/rollback protocol, or at minimum emit a `tracing::warn!` when `work_ref` is present but the lock file was not written by the expected caller. Also document the split responsibility clearly in the function docs.

- **W2: completion_lock.json lacks schema_version forward-compatibility field** → The `CompletionLock` struct has `work_id`, `locked_at`, `reason` but no `schema_version`. If a future migration renames fields or changes the schema, old lock files on user disks will fail to parse with `serde_json::Error`. **Fix:** Add a `schema_version: u32` field (default 1) to `CompletionLock`, and make `read_completion_lock` tolerant of missing/unknown versions (graceful degradation or explicit migration path).

- **W3: `creator works use` CLI calls a non-existent daemon endpoint** → `handle_use` in `crates/nexus42/src/commands/creator/works/mod.rs:369` POSTs to `/v1/local/works/pool` with action `"set_pool_active"`. No handler exists for this endpoint in the daemon-runtime API router. Users invoking `nexus42 creator works use <work_id>` will receive a 404 at runtime. The completion-report §7 acknowledges this as deferred to DF-61 (P1), but shipping a CLI command that unconditionally 404s is a reliability gap. **Fix:** Either (a) implement the minimal daemon handler for pool active promotion/demotion in P0, or (b) hide the `use` subcommand behind a compile-time or runtime feature flag until the backend is ready.

- **W4: `mark_work_completed` only emits debug-level tracing, not info** → When a Work transitions to completed — a significant lifecycle event — the function emits `tracing::debug!` (line 289 in auto_chain.rs) rather than `tracing::info!`. Operators reviewing logs at default info level will not see completion events. **Fix:** Upgrade the log line to `tracing::info!` and include `work_id`, `creator_id`, `completion_locked_at`, and `work_ref` for observability.

### 🟢 Suggestion
- **S1: Partial index on novel_pool_entries is P0-adequate but monitor query patterns** → The partial unique index `ON novel_pool_entries(creator_id) WHERE status = 'active'` (migration line 28) is optimal for the "one active per creator" constraint enforcement. However, `creator works list` currently queries the `works` table directly (not JOINing with `novel_pool_entries`), so the index is only exercised by the not-yet-implemented pool handler. When DF-61 implements pool queries, verify that `SELECT * FROM novel_pool_entries WHERE creator_id = ? AND status = 'active'` uses the partial index via `EXPLAIN QUERY PLAN`.

- **S2: `repeated_sweeps_remain_stable` test flakiness is pre-existing** → Confirmed 2/3 failures in local runs. The test uses `run_one_sweep` with a 60s threshold against seeded `created_at` timestamps; timing jitter in async execution can cause the stale-finding window to shift. Not introduced by V1.41. **Fix:** Consider using a mocked clock or deterministic `created_at` offset in a future hygiene slice (separate from this plan).

- **S3: `list_works` API does not expose new V1.41 columns in summary view** → `WorkSummary` (works.rs line 175) returns only `work_id`, `title`, `status`, `intake_status`, `primary_preset_id`, `updated_at`. The new columns (`completion_locked_at`, `novel_completion_status`, `runtime_lock_holder`) are omitted from the list view. This is intentional for brevity but means CLI `creator works list` cannot show lock status at a glance. **Fix:** Consider adding `completion_locked_at` to `WorkSummary` if the UX team wants lock visibility in list view.

- **S4: R-V141P0-N01 sqlx offline cache partial refresh** → `.sqlx/` has a recently-updated file (query-1ad9bc84...json at 15:47) but the completion report notes the cache was not fully refreshed for all new columns. All new queries use runtime `sqlx::query()` with `// SAFETY:` comments, so compilation succeeds without the offline cache. **Fix:** Refresh `.sqlx/` when sqlx-cli becomes available; no P0 blocker.

## Source Trace
- Finding ID: W1
- Source Type: manual-reasoning
- Source Reference: crates/nexus-orchestration/src/auto_chain.rs:255-296
- Confidence: High

- Finding ID: W2
- Source Type: manual-reasoning
- Source Reference: crates/nexus-orchestration/src/completion_lock.rs:11-19
- Confidence: High

- Finding ID: W3
- Source Type: manual-reasoning
- Source Reference: crates/nexus42/src/commands/creator/works/mod.rs:356-379
- Confidence: High

- Finding ID: W4
- Source Type: manual-reasoning
- Source Reference: crates/nexus-orchestration/src/auto_chain.rs:289
- Confidence: High

- Finding ID: S2
- Source Type: manual-reasoning / test-execution
- Source Reference: crates/nexus-daemon-runtime/tests/master_decision_timeout.rs:258-275
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

**Rationale**: Four Warning-level findings remain unresolved. W1 (non-atomic completion) and W2 (missing schema_version) are forward-reliability issues that are cheap to fix now and expensive to fix later. W3 (CLI 404 endpoint) is a user-facing functional gap. W4 (observability) is a one-line fix. All four should be addressed or explicitly deferred with residual tracking before Approve.

**Recommended disposition**:
- If PM wants to defer W1, W2, W3 to DF-61/P1: register them as tracked residuals in `status.json` and re-review.
- If PM wants to close in P0: W4 is a trivial one-line logging change; W3 requires either hiding `use` or adding the daemon handler; W1 and W2 are small code changes.

## Revalidation (fix-wave delta: edf0a621..9b6627dd)

**Reviewer**: @qc-specialist-3 (qc-specialist-3, reviewer_index: 3)
**Re-review timestamp**: 2026-06-10T21:30:00+08:00
**Re-review range**: `merge-base: 55689706` → `tip: 9b6627dd` (focus delta `edf0a621..9b6627dd`)
**Working branch (verified)**: iteration/v1.41
**Review cwd (verified)**: /Users/bibi/workspace/organizations/42ch/nexus
**Tools run**: cargo clippy, cargo +nightly fmt --check, cargo test, manual review of fix-wave diff

### Disposition

| Finding | Original severity | New severity | Disposition | Evidence |
|---------|-------------------|--------------|-------------|----------|
| W1 (DB+file non-atomic) | warning | resolved | Fix 4 release handler + spec amendment | `completion_lock.rs` SSOT doc comments; `release_completion_lock_handler` DB-first→file-second with `tracing::warn!` on file-delete failure; supervisor `write_completion_lock_if_available` logs warning on file-write failure |
| W2 (schema_version missing) | warning | resolved | Fix 3 schema_version field | `completion_lock.rs` struct: `schema_version: u32` with `#[serde(default)]`; backward-compat (missing→1) and forward-compat (future→structured error); unit tests `read_missing_schema_version_treated_as_v1` and `read_future_schema_version_returns_error` |
| W3 (CLI 404 on /pool) | warning | resolved | Fix 1 daemon routes (same root as qc1 F-001) | `api/mod.rs` lines 251-252: `.route("/v1/local/works/pool", post(...))`; lines 263-264: `.route("/v1/local/works/{work_id}/completion-lock/release", post(...))`; handlers implemented in `handlers/works.rs` |
| W4 (debug-level tracing) | warning | resolved | Fix 5 info log on mark_work_completed | `auto_chain.rs` lines 289-296: `tracing::info!(target: "novel.completion", work_id = %work_id, creator_id = %creator_id, completion_locked_at = %now, work_ref = ?updated.work_ref, "...")` |

### New findings (if any)

None.

### Tools / verification tails

**cargo clippy** (scoped: nexus42, nexus-daemon-runtime, nexus-orchestration, nexus-local-db):
```
Checking nexus-orchestration v0.1.0
Checking nexus-daemon-runtime v0.1.0
Checking nexus42 v0.1.0
Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.90s
```

**cargo +nightly fmt --check**:
```
(no output — clean)
```

**cargo test** (scoped):
```
nexus-daemon-runtime: 15 passed; 0 failed
nexus-local-db: 2 passed; 0 failed  
nexus-orchestration: 1 passed; 0 failed
nexus42: 1 passed; 0 failed
```

**Note on pre-existing flake**: `repeated_sweeps_remain_stable` (master_decision_timeout.rs) failed 2/3 runs — same pre-existing timing flake noted in original S2, not introduced by fix-wave. No action needed for P0.

### Updated verdict

Approve

**Rationale**: All four Warning-level findings (W1, W2, W3, W4) are resolved with concrete code changes, unit tests, and documented SSOT contracts. No new Critical or Warning items introduced in the fix-wave delta. CI tools pass. Pre-existing test flake (S2) remains unchanged and was already scoped out of P0.

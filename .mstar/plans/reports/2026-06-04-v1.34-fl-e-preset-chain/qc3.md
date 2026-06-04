---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-04-v1.34-fl-e-preset-chain"
verdict: "Request Changes"
generated_at: "2026-06-05"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-05T00:00:00Z

## Scope
- plan_id: 2026-06-04-v1.34-fl-e-preset-chain
- Review range / Diff basis: merge-base: origin/main..HEAD on feature/v1.34-fl-e-preset-chain; 4 P2 commits
- Working branch (verified): feature/v1.34-fl-e-preset-chain
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.34-fl-e-preset-chain
- Files reviewed: 17 crate files + tests + migrations + docs
- Commit range: 6714243..1115699 (4 P2 commits)
- Tools run: cargo test -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db, cargo clippy -p nexus42 -p nexus-orchestration -p nexus-daemon-runtime -p nexus-local-db -- -D warnings

## Findings

### 🔴 Critical
_None._

### 🟡 Warning

#### W-1: Non-atomic stage advance + schedule create with no rollback

**Source**: `crates/nexus42/src/commands/creator/run.rs` lines 533-609 (T1 stage advance flow)

**Issue**: The `stage_advance` CLI command performs stage advance as **two separate API calls**:
1. `PATCH /v1/local/works/{work_id}` with `current_stage` + `stage_status=active`
2. `POST /v1/local/orchestration/schedules` to create the stage schedule

If step 2 fails (e.g., daemon restart, network error, preset not found), the Work remains in `stage_status='active'` but **no schedule exists**. There is **no rollback** of the PATCH. The code acknowledges this with a warning (`eprintln!("Warning: failed to create stage schedule: {e}")`) but does not attempt to revert the stage.

**Impact**: The work is stuck in "active" state with no schedule. The `has_active_fl_e_schedule()` check (which reads `stage_status`) would return `true`, preventing the user from re-advancing. The user would need to manually PATCH `stage_status` back or use `--force`.

**Evidence**:
```rust
// PATCH succeeds
let updated: serde_json::Value = client
    .patch::<serde_json::Value, _>(&format!("/v1/local/works/{work_id}"), &patch)
    .await?;

// Schedule creation is best-effort, non-atomic
match client
    .post::<serde_json::Value, _>("/v1/local/orchestration/schedules", &schedule_body)
    .await
{
    Ok(sched_resp) => { ... }
    Err(e) => {
        eprintln!("Warning: failed to create stage schedule: {e}");
        // No rollback of PATCH
    }
}
```

**Fix**: Either:
- (Preferred) Make the daemon `PATCH` handler also create the schedule atomically within the same transaction, returning the schedule ID in the response; OR
- On schedule creation failure, automatically PATCH `stage_status` back to the previous status (requires the CLI to track the pre-advance state).

---

#### W-2: Missing concurrency / TOCTOU e2e tests at API level

**Source**: `crates/nexus-orchestration/tests/fl_e_chain_demo.rs`, `crates/nexus-daemon-runtime/tests/works_api.rs` (T3 test coverage)

**Issue**: While `advance_work_stage_atomic()` in `nexus-local-db` correctly wraps read-check-update in a SQLite transaction (R-FL-E-07 fix), **there are no tests that exercise this with concurrent daemon API requests**. The `fl_e_chain_demo.rs` tests are pure unit tests for `check_stage_advance()` logic. The `works_api.rs` tests cover PATCH stage fields but do not test:
- Two concurrent PATCH requests racing to advance the same work
- Whether the transaction actually prevents double-advance under load

**Impact**: The TOCTOU fix is present in code but unverified at the integration level. A regression in the transaction wrapper (e.g., switching to a non-transactional path in a future refactor) would not be caught by existing tests.

**Fix**: Add a concurrency test in `works_api.rs` or a new integration test that:
1. Creates a work at `intake` with `stage_status=complete` and `intake_status=complete`
2. Spawns two async tasks that both PATCH to `research`/`active`
3. Asserts that exactly one succeeds and the other returns `409 Conflict` (or equivalent)

---

#### W-3: Missing audit logs for schedule creation across all 4 paths

**Source**: `crates/nexus42/src/commands/creator/run.rs` (4 schedule create paths)

**Issue**: The assignment asks: "4 个 schedule create 路径是否有 audit log?"

The 4 schedule create paths are:
1. `RunCommand::Start` → intake schedule (lines 183-198)
2. `RunCommand::Start` → novel-writing schedule (lines 225-238)
3. `stage_advance` → stage schedule (lines 593-608)
4. Daemon-side schedule creation (not in diff scope, but any future daemon auto-scheduling)

**None of these 4 paths log schedule creation events**. Only the `--force` flag usage (line 545-554) and the daemon PATCH stage update (`tracing::info!(target: "fl_e.audit", ...)`) are logged. The actual schedule creation (POST to `/v1/local/orchestration/schedules`) is not audited.

**Impact**: No observability trail for when schedules are created, by whom, for which work, and with which preset. This makes debugging "missing schedule" issues (e.g., W-1 above) difficult.

**Fix**: Add `tracing::info!(target: "fl_e.audit", ...)` before each schedule creation POST, logging:
- `work_id`
- `preset_id`
- `stage` (for stage schedules)
- `creator_id`
- `schedule_id` (on success)

---

#### W-4: CLI stage advance loses machine-readable error codes

**Source**: `crates/nexus42/src/commands/creator/run.rs` lines 530-531, `crates/nexus-orchestration/src/stage_gates.rs`

**Issue**: The daemon API preserves structured error codes (e.g., `INVALID_STAGE` for unknown stages, `CONFLICT` for active schedule). However, the CLI `stage_advance` maps all `StageGateError` variants to a plain string:

```rust
stage_gates::check_stage_advance(&work_state, target_stage, force)
    .map_err(|e| crate::errors::CliError::Other(e.message))?;
```

The `StageGateError` struct only has a `message: String` field — no `code` field. This means CLI consumers cannot programmatically distinguish between:
- Unknown stage
- Backwards advance
- Skip without force
- Active schedule exists
- Incomplete current stage

**Impact**: CLI automation (scripts, CI) must parse human-readable error messages to handle specific errors, which is brittle.

**Fix**: Add a `code: String` field to `StageGateError` (e.g., `UNKNOWN_STAGE`, `BACKWARDS_ADVANCE`, `STAGE_SKIP`, `ACTIVE_SCHEDULE`, `INCOMPLETE_STAGE`) and propagate it through `CliError`. Alternatively, map the gate error messages to specific `CliError` variants.

### 🟢 Suggestion

#### S-1: `default_preset_for_stage` implicit panic risk

**Source**: `crates/nexus-orchestration/src/preset/validation.rs`

**Issue**: `default_preset_for_stage` uses `presets[0]` which will panic if an allowlist entry ever has an empty presets array:

```rust
.map(|(_, presets)| presets[0])
```

While the current `STAGE_PRESET_ALLOWLIST` always has at least one preset per stage, this is an implicit invariant not enforced by the type system.

**Fix**: Use `.and_then(|(_, presets)| presets.first().copied())` to return `None` safely on empty arrays, or add a `debug_assert!(!presets.is_empty())` at the const definition site.

#### S-2: Sequential API round-trips add latency

**Source**: `crates/nexus42/src/commands/creator/run.rs` `stage_advance`

**Issue**: `stage_advance` does GET work → PATCH work → POST schedule sequentially. For a local daemon this is negligible, but if the daemon is ever remote (or under high load), this is 3 round-trips where 1 could suffice.

**Fix**: (Future) Consider a daemon endpoint like `POST /v1/local/works/{id}/advance-stage` that performs gate validation + stage update + schedule creation atomically and returns both the updated work and schedule ID.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|-----------|-------------|------------------|------------|
| W-1 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs:533-609` | High |
| W-2 | manual-reasoning | `crates/nexus-orchestration/tests/fl_e_chain_demo.rs` (no concurrent tests) | High |
| W-3 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs` (4 schedule POST sites, 0 audit logs) | High |
| W-4 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs:530-531`, `crates/nexus-orchestration/src/stage_gates.rs:67-71` | High |
| S-1 | manual-reasoning | `crates/nexus-orchestration/src/preset/validation.rs` (diff) | Medium |
| S-2 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs:500-609` | Low |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

Rationale: W-1 (non-atomic advance+schedule with no rollback) is a reliability gap that can leave works in a stuck state. W-3 (missing audit logs) is an observability gap that compounds debugging of W-1. W-4 (lost error codes) is a CLI/UX issue. W-2 (missing concurrency tests) is a test coverage gap for the TOCTOU fix.

All warnings should be addressed before approval:
- W-1: Either make schedule creation atomic with stage advance, or implement rollback on failure
- W-2: Add concurrent PATCH test to verify R-FL-E-07 at API level
- W-3: Add audit logging to all 4 schedule create paths
- W-4: Add error codes to `StageGateError` and propagate through CLI

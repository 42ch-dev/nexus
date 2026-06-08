---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-09-v1.39-fl-e-auto-chain-engine"
verdict: "Approve"
generated_at: "2026-06-09"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-09T18:00:00Z

## Scope
- plan_id: 2026-06-09-v1.39-fl-e-auto-chain-engine
- Review range / Diff basis: merge-base: c7a3fac1 (iteration/v1.39) + tip: c143da1f (feature/v1.39-fl-e-auto-chain-engine HEAD); equivalent to git diff c7a3fac1...c143da1f
- Working branch (verified): feature/v1.39-fl-e-auto-chain-engine
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.39-p0
- Files reviewed: 14
- Commit range: c7a3fac1..c143da1f (15 commits, +2034 / -54)
- Tools run: cargo clippy --all -- -D warnings, cargo test -p nexus-orchestration --test auto_chain, cargo test -p nexus-local-db, cargo test -p nexus-daemon-runtime, cargo +nightly fmt --all -- --check

## Findings

### 🔴 Critical

None.

### 🟡 Warning

#### W-1: `resume_auto_chain_work` duplicates `enqueue_auto_chain_step` (~40 lines)

- **File**: `crates/nexus-daemon-runtime/src/boot.rs:591-633` vs `crates/nexus-orchestration/src/schedule/supervisor.rs:463-531`
- **Commit**: `e1766b07` (boot.rs) and `1cd73b31` (supervisor.rs)
- **Issue**: The boot recovery helper `resume_auto_chain_work` is a near-identical copy of `ScheduleSupervisor::enqueue_auto_chain_step`. Both construct a schedule request via `build_auto_chain_schedule`, insert a pending schedule row, and call `set_driver`. The only difference is that the boot path runs before the supervisor is fully wired.
- **Risk**: Future changes to the schedule insert logic (e.g., adding new columns, changing the schedule ID format, adding validation) must be applied in two places. This is a classic copy-paste maintenance hazard.
- **Fix**: Extract the shared schedule-insert + set-driver logic into a public function in `auto_chain.rs` (e.g., `enqueue_auto_chain_schedule(pool, creator_id, work_id, stage, chapter, work) -> Result<String, AutoChainError>`) and call it from both the supervisor and the boot path. The boot path can then be reduced to calling this shared function.

#### W-2: Schedule ID uses `ACH{timestamp}` instead of ULID

- **File**: `crates/nexus-orchestration/src/schedule/supervisor.rs:485`, `crates/nexus-daemon-runtime/src/boot.rs:605`
- **Commit**: `1cd73b31`, `e1766b07`
- **Issue**: Auto-chain schedule IDs are generated as `ACH{YYYYMMDDHHMMSSmmm}` (e.g., `ACH20260609180000123`). The rest of the codebase uses ULIDs for schedule IDs. Under high-frequency auto-chain transitions (e.g., rapid stage completions in the same millisecond), this format could produce collisions.
- **Risk**: Low probability but non-zero. Two schedules completing in the same millisecond (e.g., intake → research transition and a concurrent boot resume) could generate identical IDs, causing a primary-key violation on `creator_schedules`.
- **Fix**: Use ULID generation (consistent with the rest of the codebase) or append a random suffix. The `ACH` prefix is useful for observability; consider `ACH_<ulid>`.

#### W-3: `creator run resume` only clears `auto_chain_interrupted` without triggering re-evaluation

- **File**: `crates/nexus42/src/commands/creator/run.rs:812-849`
- **Commit**: `f04b16c4`
- **Issue**: The `creator run resume <work_id>` command PATCHes `auto_chain_interrupted: false` but does NOT call `evaluate_next_step` or enqueue a schedule. The actual re-evaluation depends on the daemon's next `tick()` cycle. If the daemon is running but no tick is triggered (e.g., no other schedule transitions), the resumed work may sit idle indefinitely.
- **Risk**: User runs `creator run resume`, sees "auto-chain resumed" message, but the chain doesn't actually advance until some other event triggers a tick. This is a UX gap — the user expects immediate action.
- **Fix**: After clearing `auto_chain_interrupted`, the resume command should either (a) trigger a supervisor tick via a new daemon endpoint, or (b) directly call `evaluate_next_step` + enqueue the next schedule via the daemon API. Alternatively, document the boot-resume precondition in the CLI help text: "The daemon will evaluate the next step on its next tick cycle. Run `creator run status <work_id>` to confirm."

### 🟢 Suggestion

#### S-1: `pending_inspiration` count not surfaced in `creator run status` UX

- **File**: `crates/nexus42/src/commands/creator/run.rs:568-781`
- **Commit**: `49a86c61`
- **Issue**: The spec (creator-workflow.md §5.5, iteration compass §2.2) mentions `pending_inspiration` as a status field. The `inspiration_log` is available via the API but the status UX does not show a count of pending/unconsumed inspiration entries.
- **Fix**: Add an `inspiration_count` line to the status output, e.g., `inspiration: 3 pending notes`. This is a low-effort, high-visibility improvement for the side-input lane UX.

#### S-2: Boot resume ordering invariant is implicit

- **File**: `crates/nexus-daemon-runtime/src/boot.rs:205-341`
- **Commit**: `e1766b07`
- **Issue**: The boot sequence runs auto-chain recovery (lines 226-341) BEFORE the supervisor's `resume_running_as_paused` (lines 205-220). This ordering is correct (auto-chain works get their schedules enqueued before the blanket pause), but it is not documented as an invariant. A future refactor that reorders these sections could break the auto-resume behavior.
- **Fix**: Add a comment block before the auto-chain recovery section: "MUST run before resume_running_as_paused — auto-chain schedules are inserted as 'pending' and must be enqueued before the supervisor pauses remaining 'running' schedules."

#### S-3: 409 error message for side-input could be more CLI-friendly

- **File**: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:651-657`
- **Commit**: `99102ec4`
- **Issue**: The 409 error message for side-input during active auto-chain is technically correct but verbose (3 lines, 200+ chars). When displayed by the CLI, it wraps awkwardly.
- **Fix**: Shorten to a single actionable line: "Auto-chain is active (driver: {id}). Side input is blocked until the current stage completes. Use `creator run status {work_id}` to check progress."

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-1 | git-diff + manual-reasoning | `boot.rs:591-633` vs `supervisor.rs:463-531` | High |
| W-2 | manual-reasoning | `supervisor.rs:485`, `boot.rs:605` | Medium |
| W-3 | manual-reasoning | `run.rs:812-849` | Medium |
| S-1 | spec-vs-implementation | `creator-workflow.md §5.5` vs `run.rs:568-781` | High |
| S-2 | manual-reasoning | `boot.rs:205-341` | Medium |
| S-3 | manual-reasoning | `works.rs:651-657` | Low |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

### Architecture-Level Observations

1. **Good — `evaluate_next_step` is a well-isolated pure function.** The decision logic in `auto_chain.rs` has no side effects, no DB access, and is exhaustively unit-tested (15 tests). The supervisor and boot path both consume it without modification. This is the right separation.

2. **Good — Single FL-E driver invariant is correctly enforced.** The side-input gate in `append_inspiration` (works.rs:647-658) checks both `auto_chain_enabled` AND `driver_schedule_id.is_some()` before rejecting. The boot resume path and the supervisor terminal hook both respect the invariant — at most one active driver schedule per Work.

3. **Concern — Code duplication between boot path and supervisor.** The `resume_auto_chain_work` helper in boot.rs is a near-copy of `enqueue_auto_chain_step` in supervisor.rs (W-1). This is the most actionable maintainability risk in the diff. Extracting the shared logic would reduce the duplication from ~40 lines to a single function call.

### Acceptance Criteria Verification

| AC | Description | Status |
|----|-------------|--------|
| AC1 | `creator run start` auto-chains intake→research→produce→review→persist for chapter 1 | ✅ Verified via `ac1_full_stage_chain_intake_to_persist` test |
| AC2 | After chapter N persist, produce for N+1 enqueues automatically | ✅ Verified via `ac2_persist_chapter1_starts_chapter2` test |
| AC3 | Work completion stops all further auto-enqueue | ✅ Verified via `ac3_persist_last_chapter_marks_complete` test |
| AC4 | Daemon restart auto-resumes checkpointed auto-chain driver | ✅ Verified via `fix2_boot_resume_enqueues_next_schedule` test |
| AC5 | `creator run continue --note` during active chain does NOT create second driver | ✅ Verified via side-input 409 gate in `append_inspiration` handler |
| AC6 | `--no-auto-chain` disables automatic enqueue; checkpoint still written | ✅ Verified via `ac6_auto_chain_disabled_no_action` + `ac6_checkpoint_fields_persisted_in_db` tests |

### Branch Discipline

✅ The diff is on `feature/v1.39-fl-e-auto-chain-engine` (verified). All 14 changed files are within allowed P0 scope — no P0.5/P1/P2/P3/P4/P5 scope creep detected.

### Regression Risk

✅ V1.38 chapter selection, status UX, and works API are not affected. The migration is additive-only (3 new columns with safe defaults). The `WorkPatch` struct adds 3 `Option` fields that default to `None`, so existing PATCH callers are unaffected. The `WorkApiDto` adds 3 fields that serialize naturally.

### Verification Evidence

```
cargo clippy --all -- -D warnings  →  clean (0 warnings)
cargo test -p nexus-orchestration --test auto_chain  →  21 passed, 0 failed
cargo test -p nexus-local-db  →  157 passed, 0 failed (all test binaries)
cargo test -p nexus-daemon-runtime  →  265 passed, 0 failed, 1 ignored (all test binaries)
cargo +nightly fmt --all -- --check  →  clean (no output)
```

**Verdict**: Approve

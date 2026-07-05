---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-11-v1.42-agent-tool-production-wiring"
verdict: "Approve"
generated_at: "2026-06-12"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p6
- Review Perspective: Performance and reliability risk (DF-47 production caller wiring, hot-path overhead, async executor safety, resource lifecycle)
- Report Timestamp: 2026-06-12T00:00:00Z

## Scope
- plan_id: 2026-06-11-v1.42-agent-tool-production-wiring
- Review range / Diff basis: merge-base: 11f8079ae6df8b861ed608ede57ee628f3f3b97e (iteration/v1.42 HEAD) | tip: 4798ff6417ac0ddc80f0886f7f68d931458010aa (feature/v1.42-agent-tool-wiring HEAD) | equivalent: git diff 11f8079a..4798ff64
- Working branch (verified): feature/v1.42-agent-tool-wiring
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p3
- Files reviewed: 11 (4 implementation commits: b6e33d2a, eb56d7a2, c8a0f840, 4798ff64)
- Commit range: 11f8079a..4798ff64 (exactly matches Assignment)
- Tools run:
  - `cargo +nightly fmt --all -- --check` (clean)
  - `cargo clippy -p nexus-daemon-runtime -p nexus-orchestration -p nexus-agent-host -- -D warnings` (clean on changed crates; 1 pre-existing unused import warning in existing `agent_tool_api.rs` test)
  - `cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring --test agent_tool_api` (31 tests passed: 5 new E2E + 26 existing)

## Findings

### 🔴 Critical
- None.

### 🟡 Warning

- **W-01: Production path incomplete — `HostToolCallTask` is dead code in production**
  The stated plan goal is "wire ONE production path: real schedule tick → `HostToolCallTask` → `DaemonToolDispatchAdapter` → `HostToolExecutor::dispatch_for_schedule` → handler." The diff implements the task (`HostToolCallTask`), the adapter (`DaemonToolDispatchAdapter`), and the boot wiring (`WorkspaceState::set_daemon_tool_dispatch`), but **no production code actually instantiates or invokes `HostToolCallTask`**:
  - The preset manifest `EnterAction` enum has only `Capability` and `InnerGraph` variants; there is no host-tool action type (`crates/nexus-contracts/src/local/orchestration/preset.rs:180`).
  - `build_wired_outer_graph` (the production graph constructor) does not reference `HostToolCallTask` (`crates/nexus-orchestration/src/preset/loader.rs:888`).
  - The schedule supervisor `tick_inner` admits schedules to running status but never creates or executes tool tasks (`crates/nexus-orchestration/src/schedule/supervisor.rs:164`).
  - `CapabilityRuntimeDeps::daemon_tool_dispatch` is declared but never consumed by any capability or task in the orchestration crate (`crates/nexus-orchestration/src/capability/mod.rs:91`).
  - `HostToolCallTask` is instantiated only in the 5 new E2E tests.

  **Impact**: The infrastructure is built and tested, but the "production caller gap" is not closed. A schedule tick cannot currently invoke a `nexus.*` tool through this path because the orchestration engine has no mechanism to emit a `HostToolCallTask`. The acceptance criterion "One tool callable from a running schedule without manual CLI invocation" is not met by the code under review.

  **Fix options** (choose one):
  1. Add an `EnterAction::HostTool` variant to the preset manifest, teach `StateCompositeTask` to emit `HostToolCallTask` for it, and wire the dispatch slot from `WorkspaceState` through the engine to the composite task.
  2. Alternatively, wire `HostToolCallTask` directly into the schedule executor for a specific stage (e.g., as an explicit post-admission step).
  3. If the intent was only to build the infrastructure (adapter + task + test) and defer the actual invocation integration, update the plan AC, spec §7.4, and tracker to reflect that the **caller integration** remains open.

  **Source**: `tasks/mod.rs` (HostToolCallTask definition, no production call sites), `preset/loader.rs:888` (build_wired_outer_graph), `schedule/supervisor.rs:164` (tick_inner), `capability/mod.rs:91` (unused daemon_tool_dispatch field).
  **Confidence**: High.

- **W-02: Hot-path overhead — `HostToolCallTask::run` serializes full context and runs handlebars on every invocation**
  `HostToolCallTask::run` (the code that would execute on a schedule tick) performs:
  1. `build_nested_payload(&context)` — serializes the entire graph-flow `Context` to JSON via `serde_json::to_value`, extracts the data map, and rebuilds a nested object tree.
  2. `render_value_templates(&self.args, &payload)?` — recursively walks the args JSON and renders every string value through handlebars in strict mode.

  For `nexus.orchestration.schedule_status` (the chosen tool), args are a trivial `{"work_id": "..."}` object with no template placeholders. Yet the code still pays the full serialization + rendering cost. On a busy schedule with frequent ticks, this is non-trivial overhead.

  **Fix**: Short-circuit when `self.args` contains no `{{` placeholders, or add a fast-path that skips `build_nested_payload` + `render_value_templates` when template rendering is not needed.

  **Source**: `crates/nexus-orchestration/src/tasks/mod.rs:1513-1514`.
  **Confidence**: High.

- **W-03: `std::sync::Mutex` in async `HostToolCallTask` — executor blocking risk**
  `HostToolCallTask` stores its dispatch slot as `Arc<std::sync::Mutex<Option<Arc<dyn DaemonToolDispatch>>>>` (`DaemonDispatchSlot` type alias, `tasks/mod.rs:1440`). In `run()`, it calls `dispatch_arc.lock()` inside an async function.

  While the mutex is uncontended in practice (set once at boot and never mutated), using a synchronous mutex in async code is a reliability anti-pattern. If the mutex were ever poisoned or contended, the async executor thread would block. The graph-flow engine runs tasks in an async context; blocking the thread starves other concurrent sessions.

  **Fix**: Replace with `tokio::sync::Mutex` (if async lock needed) or `std::sync::ArcSwap` / atomic `Arc` pointer (if the slot is truly write-once). Since the slot is immutable after boot, `ArcSwap` or even a plain `Option<Arc<dyn DaemonToolDispatch>>` behind an `Arc` would suffice.

  **Source**: `crates/nexus-orchestration/src/tasks/mod.rs:1440-1442`, `1526`.
  **Confidence**: High.

### 🟢 Suggestion

- **S-01: Request ID allocation overhead — UUID v4 on every tool call**
  `HostToolCallTask::run` generates a request ID with `uuid::Uuid::new_v4()` on every invocation (`tasks/mod.rs:1517-1521`). For high-frequency schedule ticks, UUID generation is relatively expensive. Consider a cheaper scheme (monotonic counter + schedule/task ID) if this path becomes hot.

  **Source**: `crates/nexus-orchestration/src/tasks/mod.rs:1517`.
  **Confidence**: Medium.

- **S-02: Adapter error code erasure — graph logic cannot differentiate failure modes**
  `DaemonToolDispatchAdapter::dispatch_tool` maps every `NexusApiError` to `CapabilityError::Internal(format!("daemon tool dispatch failed for {tool_name}: {e}"))` (`host_tool_executor.rs:449`). This erases structured error codes (`FORBIDDEN`, `INVALID_INPUT`, `POLICY_BLOCKED`). Future graph-flow conditional routing (e.g., retry on transient DB error but fail-fast on authz denial) will not be possible without string-matching the error message.

  **Source**: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:449`.
  **Confidence**: High. (Also noted by qc2 as S-02.)

- **S-03: `DaemonToolDispatchAdapter` holds broad `WorkspaceState` clone**
  The adapter stores a full `WorkspaceState` clone (`host_tool_executor.rs:428`). While all current fields are `Arc`-wrapped (making the clone cheap), future additions of non-`Arc` fields could inadvertently make the adapter hold stale or expensive data. Consider narrowing to the specific references needed (pool, home path) when `WorkspaceState` next changes.

  **Source**: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:427-428`.
  **Confidence**: Medium. (Also noted by qc2 as S-01.)

## Source Trace

- Finding ID: W-01 (production path incomplete)
- Source Type: manual-reasoning + cross-file code review
- Source Reference: `tasks/mod.rs:1453` (HostToolCallTask struct), `preset/loader.rs:888` (build_wired_outer_graph), `schedule/supervisor.rs:164` (tick_inner), `capability/mod.rs:91` (unused field)
- Confidence: High

- Finding ID: W-02 (hot-path overhead)
- Source Type: manual-reasoning + code review
- Source Reference: `tasks/mod.rs:1513-1514`
- Confidence: High

- Finding ID: W-03 (sync mutex in async context)
- Source Type: manual-reasoning + Rust async best-practices
- Source Reference: `tasks/mod.rs:1440-1442`, `1526`
- Confidence: High

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

**Rationale**: No Critical findings remain. The original W-01 (production caller gap) and W-02 (hot-path overhead) findings were addressed by the fix wave, as detailed in `## Revalidation` below. W-03 is mitigated on the production path. The three Suggestion-level items from the initial review remain recorded but are non-blocking. Lint and the relevant integration tests are clean.

The infrastructure is sound:
- `dispatch_for_schedule` correctly creates a `ToolExecuteRequest` with `caller_kind=Schedule` and delegates to the same `HostToolExecutor::execute` used by HTTP (single dispatch table invariant per spec §7.1).
- `DaemonToolDispatchAdapter` is correctly wired at boot before any schedule tick can run.
- `HostToolCallTask` propagates `dispatch_tool` errors as `TaskExecutionFailed` (no silent swallow).
- The 6 hermetic E2E tests in `agent_tool_production_wiring` cover round-trip, stub mode, graph-flow context integration, cross-creator rejection, Schedule-vs-HTTP result equivalence, and completion-lock read-only behavior.
- All existing tests in `agent_tool_api.rs` continue to pass.

The DF-47 production caller gap is now closed in the preset-graph path. Lint and tests are clean. Report committed on the review branch per assignment.

## Revalidation

Targeted re-review of the fix wave `b122db77..HEAD` (`aa0574cc`, `8cda43c9`) on the QC worktree at `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p3-reqc` (detached HEAD at `8cda43c9`).

### W-01: Production path incomplete — RESOLVED

The fix closes the production caller gap identified in the initial review:

- `EnterAction::HostTool` variant added to the wire/preset manifest (`crates/nexus-contracts/src/local/orchestration/preset.rs:193`).
- `StateCompositeTask` gained a `daemon_tool_dispatch` slot and a `with_daemon_tool_dispatch()` builder, and its `EnterAction` handler now constructs and runs a `HostToolCallTask` for `HostTool` actions (`crates/nexus-orchestration/src/tasks/mod.rs:748`).
- `HostToolCallTask::from_dispatch()` creates the task with a direct `Arc<dyn DaemonToolDispatch>` production path, avoiding the `Mutex<Option<...>>` wrapper used only by tests (`tasks/mod.rs:1525`).
- `GraphFlowEngine` stores the dispatch and passes it into `build_wired_outer_graph()`; the loader injects it into every `StateCompositeTask` (`engine.rs:563`, `preset/loader.rs:915`).
- Daemon boot wires the `DaemonToolDispatchAdapter` into `WorkspaceState` and into the engine (`crates/nexus-daemon-runtime/src/boot.rs:134`, `172`).

The full production chain is now: preset manifest `host_tool` enter action → `StateCompositeTask` → `HostToolCallTask::from_dispatch` → `DaemonToolDispatchAdapter::dispatch_tool` → `HostToolExecutor::dispatch_for_schedule` → handler.

### W-02: Hot-path overhead — RESOLVED

`HostToolCallTask::run()` now short-circuits expensive context serialization and handlebars rendering when `self.args` contains no `{{` placeholders (`tasks/mod.rs:1579`). A new recursive `value_contains_template()` helper checks objects, arrays, and strings. For `nexus.orchestration.schedule_status` calls with a trivial `{"work_id":"..."}` object, the task now pays only a shallow JSON traversal instead of `build_nested_payload` + `render_value_templates`.

### W-03: `std::sync::Mutex` in async context — MITIGATED

The production path introduced by `HostToolCallTask::from_dispatch()` stores the dispatch as a direct `Arc<dyn DaemonToolDispatch>` and never acquires the test-only `Mutex<Option<...>>` slot. The legacy test constructor still uses the mutex, which is acceptable because it is set once and uncontended, but the hot production path no longer exercises it.

### Verification commands

```bash
# Worktree / range alignment
cd /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p3-reqc
git rev-parse --show-toplevel   # /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p3-reqc
git rev-parse --abbrev-ref HEAD # HEAD (detached)
git log b122db77..HEAD --oneline
# 8cda43c9 merge(v1.42 P3 fix-wave): production path wiring + hot-path + test strengthening
# aa0574cc fix(v1.42 P3): QC fix wave — wire production path + hot-path + test strengthening

# Lint / format
cargo +nightly fmt --all --check          # (no output = clean)
cargo clippy -p nexus-daemon-runtime -p nexus-orchestration -p nexus-agent-host -- -D warnings  # clean

# Production-path integration tests
cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring
# 6 passed; 0 failed (new tests: schedule_status through adapter, HostToolCallTask round-trip,
# cross-creator Forbidden variant, completion-lock read-only path, dispatch/execute equivalence,
# stub mode without adapter)

# The Assignment-listed target `cargo test -p nexus-orchestration --test supervisor`
# does not exist in this worktree; the production-path coverage is provided by the
# `agent_tool_production_wiring` integration test above.
```

### Updated Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

The qc3 findings that originally drove `Request Changes` (W-01 production caller gap and W-02 hot-path overhead) are resolved. Lint and the relevant integration tests are clean. Outstanding Suggestions (S-01, S-02, S-03 from the initial wave) remain recorded but are non-blocking.

## Evidence Appendix (QC verification commands)

```bash
# Worktree / branch / range alignment
cd /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p3
git rev-parse --show-toplevel   # /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p3
git branch --show-current       # feature/v1.42-agent-tool-wiring
git log 11f8079a..4798ff64 --oneline  # 4 commits, matches Assignment

# Lint / tests (required by Assignment)
cargo +nightly fmt --all -- --check          # (no output = clean)
cargo clippy -p nexus-daemon-runtime -p nexus-orchestration -p nexus-agent-host -- -D warnings  # clean (1 pre-existing warning in unrelated test)
cargo test -p nexus-daemon-runtime --test agent_tool_production_wiring --test agent_tool_api  # 31 passed (5+26)
```

All commands executed from the Assignment-specified `Review cwd` on the exact `Review range`.

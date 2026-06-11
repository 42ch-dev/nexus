---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-11-v1.42-agent-tool-production-wiring"
verdict: "Request Changes"
generated_at: "2026-06-11"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Performance and reliability risk (DF-47 production caller wiring, hot-path overhead, async executor safety, resource lifecycle)
- Report Timestamp: 2026-06-11T21:30:00Z

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
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

**Rationale**: No Critical findings. Three Warnings, two of which (W-02, W-03) are performance/reliability anti-patterns in the hot-path code that would execute on every schedule tick. However, the primary reason for `Request Changes` is **W-01**: the stated production path (schedule tick → `HostToolCallTask`) is not actually wired in production. The task type exists and is tested, but no preset loader, schedule executor, or capability registry entry invokes it. This means the plan's acceptance criterion "One tool callable from a running schedule without manual CLI invocation" is not met by the code under review.

The infrastructure is sound:
- `dispatch_for_schedule` correctly creates a `ToolExecuteRequest` with `caller_kind=Schedule` and delegates to the same `HostToolExecutor::execute` used by HTTP (single dispatch table invariant per spec §7.1).
- `DaemonToolDispatchAdapter` is correctly wired at boot before any schedule tick can run; the field is behind `Arc<Option<...>>` with a one-time `set_` at startup.
- `HostToolCallTask` propagates `dispatch_tool` errors as `TaskExecutionFailed` (no silent swallow).
- The 5 new hermetic E2E tests cover round-trip, stub mode, graph-flow context integration, cross-creator rejection, and Schedule-vs-HTTP result equivalence.
- All 26 existing tests in `agent_tool_api.rs` continue to pass.

But until `HostToolCallTask` is actually invoked by the schedule executor or preset engine, the DF-47 production caller gap is not closed. Lint and tests are clean. Report committed on the review branch per assignment.

## Revalidation (if targeted re-review)
N/A — initial wave.

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

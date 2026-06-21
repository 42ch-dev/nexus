---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-22-v1.57-daemon-refactor-and-caller-adapters"
verdict: "Request Changes"
generated_at: "2026-06-21"
---

# QC3 Review — V1.57 P1 Daemon Refactor & 3-Caller Adapters

## Reviewer Metadata

- **Reviewer**: @qc-specialist-3
- **Runtime Agent ID**: qc-specialist-3
- **Runtime Model**: deepseek/deepseek-v4-pro
- **Review Perspective**: Performance and reliability risk
- **Report Timestamp**: 2026-06-21T23:30:00Z

## Scope

- **plan_id**: 2026-06-22-v1.57-daemon-refactor-and-caller-adapters
- **Review range / Diff basis**: 56d459ec..fe501b6b
- **Working branch (verified)**: iteration/v1.57
- **Review cwd (verified)**: /Users/bibi/workspace/organizations/42ch/nexus
- **Files reviewed**: 16
- **Commit range**: 68989e03..fe501b6b (5 commits)
- **Tools run**: `cargo test -p nexus-daemon-runtime` (3×), `cargo test -p nexus42` (1×), `cargo clippy -p nexus-daemon-runtime -p nexus42 -- -D warnings`, `cargo +nightly fmt -p nexus-daemon-runtime -p nexus42 -- --check`, `wc -l`, manual diff inspection

## Summary

- **AC met**: 14 / 18
- **Findings**: 3 (1 Critical, 0 Warning, 2 Suggestion)
- **Verdict**: Request Changes

## Acceptance Criteria Checklist

| # | AC | Status | Evidence |
|---|-----|--------|----------|
| 1 | `host_tool_executor.rs` ≤ 800 lines | ✅ Met | 349 lines (`wc -l`) |
| 2 | Three caller entry points exist | ✅ Met | `execute()` (HTTP), `dispatch_from_worker()` (worker), `dispatch_for_schedule()` (schedule) + CLI `host-call` subcommand |
| 3 | All three dispatch through `CapabilityRegistry::dispatch` | ✅ Met | All paths call `registry_dispatch()` → `reg.dispatch()` at line 226 |
| 4 | 7 duplicated `execute_X` functions removed | ✅ Met | Diff confirms removal; handlers live in `host_tool_handlers.rs` + `capability/builtins/` |
| 5 | `nexus42 host-call <tool_id> --args <json>` works end-to-end | ✅ Met | Integration test `test_host_call_dispatches_through_registry_read` passes |
| 6 | `host-call --help` documents debug-only intent | ✅ Met | `cli.rs` doc comment: "Debug-only: invoke a host tool through the daemon registry"; `host_call.rs` §1 |
| 7 | `cli-spec.md` §6.2M added for `host-call` | ❌ NOT MET | Zero `.md` files changed in review range; `cli-spec.md` not modified |
| 8 | `daemon-runtime.md` host_tool section updated | ❌ NOT MET | Zero `.md` files changed in review range |
| 9 | `local-runtime-boundary.md` topology updated | ❌ NOT MET | Zero `.md` files changed in review range |
| 10 | `orchestration-engine.md` §6.4 updated | ❌ NOT MET | Zero `.md` files changed in review range |
| 11 | `CdnConfig` constructor-injected; global `RwLock` removed | ✅ Met | `RegistryRefresh { cdn_config: Option<CdnConfig> }` replaces global `static CDN_CONFIG: RwLock<Option<CdnConfig>>`; `with_cdn()` constructor added |
| 12 | `R-V156P3-S003` field drops | ✅ Met | Doc comment cleanup on `ToolExecuteRequest` fields; no unused fields remain in caller surfaces |
| 13 | 3 caller integration tests | ✅ Met | `test_host_call_dispatches_through_registry_read`, `test_worker_agent_tool_request_dispatches_through_registry`, `test_http_tool_execute_dispatches_through_registry` — all pass |
| 14 | `host-call` smoke test ≥ 3 tool IDs | ✅ Met | 3 `#[ignore]` tests (read/write/policy-gated) + 1 non-ignored test (`host_call_rejects_invalid_json`) |
| 15 | `cargo test -p nexus-daemon-runtime` passes | ✅ Met | 267 unit tests + all integration tests pass (3× no flakiness) |
| 16 | `cargo test -p nexus42` passes | ✅ Met | 762 unit tests + all integration tests pass |
| 17 | `cargo clippy -p nexus-daemon-runtime -p nexus42 -- -D warnings` passes | ✅ Met | Clean, zero warnings on touched crates |
| 18 | `cargo +nightly fmt -p nexus-daemon-runtime -p nexus42 -- --check` passes | ✅ Met | Clean, no format errors |

## Findings

### 🔴 Critical

**F-001 — Spec amendments (ACs 7–10) not delivered: zero .md files changed**

- **Plan requirement**: T6 requires spec amendments for `cli-spec.md` §6.2M, `daemon-runtime.md`, `local-runtime-boundary.md`, and `orchestration-engine.md` §6.4.
- **Evidence**: `git diff 56d459ec..fe501b6b --name-only | grep '\.md$'` returns empty. The commit `fc0e250a` ("T5-T8 — field drops, spec amendments, integration tests, smoke test") contains zero `.md` file changes. All 16 changed files are `.rs` source.
- **Impact**: The delivery compass §2 lists 4 Master spec documents requiring amendment in P1. Without these amendments, the spec SSOT is out of sync with the implementation — future implementers and reviewers cannot verify the 3-caller adapter topology, the `host-call` subcommand contract, or the updated host_tool section against canonical specs.
- **Blocking**: These are explicit acceptance criteria (AC 7, 8, 9, 10). Absence = AC not met = `Request Changes`.
- **Suggested action**: Amend the 4 spec files in a fix-wave commit before re-review.

### 🟡 Warning

_(None)_

### 🟢 Suggestion

**F-002 — `load_permission_policy` performs filesystem I/O on every tool dispatch (pre-existing, not regression)**

- **Scope**: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs` line 664–697.
- **Rationale**: The `admission_pipeline` calls `load_permission_policy` on every tool execution request, which reads and parses `permissions.toml` from disk each time. Under high concurrency (e.g., schedule-initiated tool calls at 1000 RPS), this adds filesystem pressure. This is pre-existing behavior (was present in the old god-file), not introduced by this refactor — but the refactor moves it into `host_tool_handlers.rs` without adding caching.
- **Suggested action**: Add a `tokio::sync::RwLock<Option<WorkspacePermissionPolicy>>` cache with mtime-based invalidation to the `WorkspaceState` or admission pipeline. Not blocking for this review; file as a low-severity residual.

**F-003 — Pre-existing unused import warning in `agent_tool_api.rs`**

- **Scope**: `crates/nexus-daemon-runtime/tests/agent_tool_api.rs:27` — `HostToolCallerKind` imported but unused.
- **Rationale**: Observed in test output during `cargo test -p nexus-daemon-runtime`. This file has zero changes in the review range — it is a pre-existing issue, not a regression from P1. However, since this plan touches `HostToolCallerKind` (defined in `host_tool_executor.rs`), the unused import becomes more visible.
- **Suggested action**: Remove unused import in a follow-up cleanup pass. Not blocking.

## Detailed Notes

### 1. God-file refactor runtime impact

The `host_tool_executor.rs` is reduced from 4298 to 349 lines. The dispatch flow:

```
execute() → registry_dispatch() → admission_pipeline() + CapabilityRegistry::dispatch()
```

Function-call overhead: 2 hops max (1 to `registry_dispatch`, 1 to `admission_pipeline` + `reg.dispatch()`). The old code had identical overhead — the admission pipeline was already a separate function call. The primary improvement is deduplication: 7 `execute_X` functions with repeated dispatch logic → 1 unified path. **No performance regression; structural improvement.**

The extracted `host_tool_handlers.rs` (1839 lines) contains all handler implementations and admission logic. The extracted `host_tool_executor_tests.rs` (2177 lines) is included via `#[path = "host_tool_executor_tests.rs"]` at compile time, ensuring no additional test compilation cost beyond a file-read.

### 2. 3-caller integration test runtime

All 3 integration tests run in <1s each:
- `test_host_call_dispatches_through_registry_read` — exercises CLI path via daemon IPC mock
- `test_worker_agent_tool_request_dispatches_through_registry` — exercises worker upcall path
- `test_http_tool_execute_dispatches_through_registry` — exercises HTTP POST path

`test_dispatch_equivalence_all_three_paths` (AC 13) additionally verifies that the same tool_id + input → same output across all 3 paths. All pass. No test runtime regression.

### 3. `host-call` smoke test runtime

Three tests are `#[ignore]` with appropriate reasons:
- `host_call_smoke_read_tool` — requires running daemon with active creator
- `host_call_smoke_write_tool` — requires running daemon with active creator and valid work_id
- `host_call_smoke_policy_gated_tool` — requires running daemon

One non-ignored test (`host_call_rejects_invalid_json`) validates JSON parsing. No silent breakage.

### 4. CdnConfig `Option<...>` runtime cost

**Constructor-injected, not global state.** The `RegistryRefresh` struct now holds `cdn_config: Option<CdnConfig>`:

```rust
// registry.rs line 283-284
if let Some(ref cdn) = self.cdn_config {
    match fetch_from_cdn(cdn).await { ... }
}
```

- Checked with `if let Some(ref cdn)` — safe pattern matching. Zero panic risk on `None`.
- Config is cloned at daemon boot (`boot.rs` lines 125–139), not per-request.
- Global `RwLock<Option<CdnConfig>>` fully removed (ex-`set_cdn_config()` / `get_cdn_config()`).
- Test isolation improved: `serial_test::serial` no longer needed for CDN config reset (was `reset_cdn_config()` in every test). Tests now create independent `RegistryRefresh` instances.

**No runtime overhead regression. Constructor injection is safer (no poisoned-lock panic surface).**

### 5. Field drops in caller surfaces

`ToolExecuteRequest` retains 5 fields, all actively used. Doc comment cleanup removed duplicated documentation but preserved all fields. `WorkerToolResult` uses `#[serde(skip_serializing_if = "Option::is_none")]` to avoid serializing absent optional fields — zero-cost. `CliHostCallArgs` is minimal (2 fields: `tool_id`, `args`).

No unused fields allocated. Memory footprint unchanged from pre-refactor.

### 6. Flakiness check

`cargo test -p nexus-daemon-runtime` executed 3×:
- Run 1: 267 passed, 0 failed, 16.75s
- Run 2: 267 passed, 0 failed, 17.20s
- Run 3: 267 passed, 0 failed, 16.35s

All integration test binaries also passed all 3 runs. **Zero flakiness detected.**

### 7. Regression in lib tests

Extracted tests in `host_tool_executor_tests.rs` (2177 lines) run as part of the standard unit test suite via `#[path = "host_tool_executor_tests.rs"]`. All 267 unit tests pass. The test runtime (~16.75s for 267 tests) is comparable to pre-refactor baselines (no significant change).

### 8. Process spawn cost for host-call

Each `nexus42 host-call <tool_id> --args <json>` invocation:
1. Spawns the full `nexus42` binary (process creation + clap parse + config load)
2. Creates a `DaemonClient` (no connection pooling)
3. Makes one HTTP POST to the daemon (`/v1/local/agent-host/internal/tool-executions`)
4. Prints JSON response and exits

No connection caching or daemon session reuse. This is **acceptable for a debug-only subcommand** — the CLI help explicitly documents "Debug-only" intent. Adding persistent daemon connections would be scope creep.

### 9. Capacity: 3 caller paths scale

`CapabilityRegistry` uses:
- `HashMap<&'static str, usize>` index → O(1) lookup
- `Arc` shared, read-only after construction → zero lock contention on lookup
- `Vec<Box<dyn Capability>>` ordered storage → minimal indirection (one pointer dereference)

The 3 caller entry points are normalizers only — wire format → `ToolExecuteRequest` → `registry_dispatch()`. No cross-caller contention. The admission pipeline (`admission_pipeline`) is the only shared path, and it has no mutable state contention.

At theoretical 1000 RPS through 3 paths, the only bottleneck is `load_permission_policy` (filesystem I/O per call — see F-002). This is pre-existing, not introduced by P1.

### 10. Pre-existing warnings

- `unused import: HostToolCallerKind` in `tests/agent_tool_api.rs:27` — pre-existing (zero diff to this file). Not a regression.
- `unused axum::Json that must be used` in `tests/findings_api.rs:185` — pre-existing. Not a regression.

## Verdict

**Request Changes** — F-001 (Critical): ACs 7–10 (spec amendments for `cli-spec.md` §6.2M, `daemon-runtime.md`, `local-runtime-boundary.md`, `orchestration-engine.md` §6.4) are not delivered. Zero `.md` files changed in the review range despite the plan requiring these amendments in T6. The implementation code is clean and passes all tests, but the spec SSOT is out of sync with the refactored code. Amend the 4 spec files in a fix-wave commit before re-review.

All 14 other ACs are met. No performance regressions detected. No flaky tests. CdnConfig constructor injection removes a global-lock panic surface. Test isolation improved (no more `reset_cdn_config()` calls). The god-file refactor to 349 lines is surgical and well-structured.

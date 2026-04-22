---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-04-21-v1.7-ws-e-multi-agent-worker"
verdict: "Request Changes"
generated_at: "2026-04-22"
---

# QC Review #1 — Multi-Agent Worker (WS-E)

**Reviewer**: @qc-specialist (#1/3)
**Plan**: `2026-04-21-v1.7-ws-e-multi-agent-worker`
**Scope**: `git diff feature/v1.7...HEAD` — 34 commits, 38 files, +8587/-675 lines
**Working branch**: `feature/v1.7-ws-e`
**Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`

---

## Summary

The WS-E multi-agent worker implementation introduces a well-structured IPC multiplexing layer (`IpcClient`), a clean session state machine (`AgentSlot`), and a registry-based worker manager (`WorkerRegistry`). The transport abstraction (`RpcTransport` + split halves) is correctly designed for concurrent use. Test coverage for core IPC and worker registry paths is solid.

However, several issues prevent an `Approve` verdict:

- **W1 (Warning)**: `handle_initialize` re-init behavior silently accumulates sessions instead of replacing them (idempotency claim is inaccurate).
- **W2 (Warning)**: `resolve_agent_model` drops agent info from preset `recommended_models`, returning only model — inconsistent with the "agent:model" format.
- **S1 (Suggestion)**: `WorkerHandle::from_ipc_for_test` has duplicated doc comments.
- **S2 (Suggestion)**: `build_inner_graphs` converts `None` agent to `Some("")` causing unnecessary lookup.

---

## Review Scope (verbatim from Assignment)

| Field | Value |
|-------|-------|
| `plan_id` | `2026-04-21-v1.7-ws-e-multi-agent-worker` |
| `Review range / Diff basis` | `git diff feature/v1.7...HEAD` (34 commits, 38 files, +8587/-675 lines) |
| `Working branch` | `feature/v1.7-ws-e` |

### Files reviewed (production code)

| File | Lines | Focus |
|------|-------|-------|
| `crates/nexus-orchestration/src/worker/transport.rs` | 322 | Split transport traits, DuplexTransport |
| `crates/nexus-orchestration/src/worker/ipc.rs` | 448 | IpcClient multiplex, dispatch_response |
| `crates/nexus-orchestration/src/worker/manager.rs` | 1045 | WorkerHandle, WorkerAgentConfig, WorkerManager |
| `crates/nexus-orchestration/src/worker/registry.rs` | 379 | WorkerRegistry, WorkerSpawner, MockSpawner |
| `crates/nexus-orchestration/src/worker/mod.rs` | 31 | Re-exports |
| `crates/nexus-orchestration/src/tasks/mod.rs` | 1303 | AcpPromptTask, InnerGraphNodeTask, session routing |
| `crates/nexus-orchestration/src/preset/loader.rs` | 1472 | Role validation, inner graph agent refs |
| `crates/nexus-orchestration/src/preset/mod.rs` | 299 | Embedded presets |
| `crates/nexus-contracts/src/local/orchestration/preset.rs` | 206 | PresetRoleDefinition DTOs |
| `crates/nexus42/src/commands/acp_worker/mod.rs` | 1829 | MultiplexedWorkerState, 9 IPC handlers |
| `crates/nexus42/src/commands/acp_worker/agent_slot.rs` | 568 | AgentSlot state machine |
| `crates/nexus42/src/config.rs` | 1232 | UserAgentsConfig, parse_agent_ref, resolve_agent_model |
| `crates/nexus42/src/commands/schedule.rs` | 1006 | New CLI flags (--agent-ref, --model, --role) |
| `crates/nexus42d/src/lifecycle/subsystems/worker_mgr.rs` | 204 | WorkerMgrSubsystem |

### Files reviewed (tests)

| File | Lines |
|------|-------|
| `crates/nexus-orchestration/tests/ipc_client_multiplex.rs` | 208 |
| `crates/nexus-orchestration/tests/multi_agent_worker.rs` | 1020 |
| `crates/nexus-orchestration/tests/worker_registry.rs` | 276 |
| `crates/nexus-orchestration/tests/preset_agents.rs` | 545 |

### Files excluded (per constraints)

- `.claude/skills/gitnexus/` — unrelated skill files
- `AGENTS.md` — project config
- `auth::tests::get_returns_none_for_unknown_creator` — pre-existing flaky test

---

## Findings

### W1 (Warning): `handle_initialize` re-init accumulates sessions, does not replace

**File**: `crates/nexus42/src/commands/acp_worker/mod.rs`
**Lines**: 300–362

The `handle_initialize` function claims "idempotent re-init" in its comment (line 301), but when sessions already exist, it **adds** new sessions to the existing map instead of **replacing** them:

```rust
// Line 301: comment says "idempotent re-init"
if !sessions.is_empty() {
    warn!(count = sessions.len(), "worker/initialize called with existing sessions");
}
// Lines 309–346: loops agents array and inserts — does NOT clear first
```

If the daemon sends `worker/initialize` twice with different agent sets, both sets will coexist in the map. The test `initialize_idempotent_replaces_sessions` (mod.rs:1440) simulates a clear-and-rebuild pattern locally but the actual handler does **not** call `sessions.clear()` before inserting.

**Impact**: Under unexpected re-initialization (daemon restart, retry logic), stale agent sessions accumulate and may receive routed prompts intended for the new session set.

**Fix**: Add `sessions.clear()` before the agents array loop, or change the comment to reflect the additive behavior and document it as such.

---

### W2 (Warning): `resolve_agent_model` discards agent info from preset `recommended_models`

**File**: `crates/nexus42/src/config.rs`
**Lines**: 1171–1173

```rust
// 3. Preset recommended_models[0] — model only (no agent override from preset)
let model = preset_recommended.first().cloned();
(None, model)
```

The `recommended_models` entries use the format `"agent_id:model_name"` (validated by `validate_recommended_model_format` in loader.rs). But `resolve_agent_model` returns the entire string as `model` and `None` as `agent`. When no CLI override or user config exists, the caller receives `agent = None` and `model = Some("claude-acp:claude-sonnet-4-20250514")` — the model field contains a colon-separated pair, not just a model name.

**Impact**: Downstream code that expects `model` to contain only the model name (e.g., `--model` flag value) will receive the full `"agent:model"` string. The agent portion is silently lost.

**Fix**: Split `preset_recommended[0]` on `':'` to extract `(Some(agent), Some(model))` consistently with other layers. Or rename the return to clarify the preset layer returns the combined string.

---

### S1 (Suggestion): Duplicated doc comment on `WorkerHandle::from_ipc_for_test`

**File**: `crates/nexus-orchestration/src/worker/manager.rs`
**Lines**: 323–332

The doc comment block is repeated verbatim:

```rust
/// Create a `WorkerHandle` from an [`IpcClient`] for testing.
/// ... (7 lines)
/// Create a `WorkerHandle` from an [`IpcClient`] for testing.
/// ... (7 lines, identical)
```

**Impact**: Cosmetic only, but indicates copy-paste during development.

---

### S2 (Suggestion): `build_inner_graphs` converts `None` agent to `Some("")`

**File**: `crates/nexus-orchestration/src/preset/loader.rs`
**Line**: 641

```rust
.with_agent_ref(node.agent.clone().unwrap_or_default())
```

`InnerGraphNodeTask::agent_ref` is `Option<String>`, but this code passes `Some("")` when `node.agent` is `None`. In `resolve_session_id` (tasks/mod.rs:713–728), the `if let Some(ref agent)` check will match the empty string, perform a lookup in `_session_routes`, find nothing, and fall through to "default".

**Impact**: Wasteful lookup for every node without an agent reference. Functionally correct but adds an unnecessary HashMap lookup per prompt.

**Fix**: Pass `node.agent.clone()` directly (preserving `None`), or add an early return for empty strings in `resolve_session_id`.

---

### S3 (Suggestion): `handle_agent_crash` is `#[allow(dead_code)]` with no test coverage

**File**: `crates/nexus42/src/commands/acp_worker/mod.rs`
**Lines**: 776–814

The `handle_agent_crash` function is marked `#[allow(dead_code)]` and has no unit test in the `#[cfg(test)]` module of `mod.rs`. The comment says "Currently it is infrastructure for crash detection; tests use `AgentSlot::simulate_crash` to exercise this code path" — but `simulate_crash` only tests the `AgentSlot` state machine, not the `handle_agent_crash` IPC handler itself (notification emission, unknown-session handling).

**Impact**: The crash notification path (`emit_session_event` with `"crashed"`) is untested in the worker handler context.

**Recommendation**: Add at least one unit test exercising `handle_agent_crash` for both found and not-found sessions.

---

### S4 (Suggestion): `AcpPromptTask` handle take/return pattern — potential loss on panic

**File**: `crates/nexus-orchestration/src/tasks/mod.rs`
**Lines**: 956–985

The worker handle is taken out of `Arc<Mutex<Option<WorkerHandle>>>` via `guard.take()` before the IPC call, then put back afterward. If `handle.call_json_rpc(...)` panics (not returns `Err`, but actually panics), the guard scope at line 981–985 is never reached and the handle is permanently lost.

**Current code flow**:
```rust
let handle = guard.take().ok_or_else(...)?;  // line 965
let ipc_result = handle.call_json_rpc(...).await;  // line 978
// If call_json_rpc panics here, the handle is never put back:
let mut guard = handle_arc.lock().map_err(...)?;  // line 982
*guard = Some(handle);  // line 984
```

**Impact**: In the extremely unlikely event of a panic inside `call_json_rpc`, subsequent prompts would fail with "worker handle consumed or not available". In practice, `call_json_rpc` returns `Result` not panic, so this is a theoretical concern.

**Recommendation**: Use `std::panic::catch_unwind` + `Drop` guard, or document the assumption that `call_json_rpc` never panics.

---

## Positive Observations

### Architecture & Design

1. **Transport split design** (`transport.rs`): The `RpcTransportRead` / `RpcTransportWrite` split traits are clean. The `Box<Self>` pattern in `split()` correctly transfers ownership and avoids cloning.

2. **IpcClient multiplexing** (`ipc.rs`): The background reader + `PendingMap` pattern is correctly implemented. `CancellationToken` integration for clean shutdown is well-designed. `IpcClient` correctly implements `Send + Sync` (verified by compile-time assertion test at `ipc_client_multiplex.rs:205`).

3. **AgentSlot state machine** (`agent_slot.rs`): State transitions are well-documented with ASCII diagram. The `mark_ready_from_prompt` guard (only transitions from `Prompting`, preserving `Error` state) is correct defensive coding (line 296–301).

4. **Lock discipline**: `MultiplexedWorkerState` uses `std::sync::RwLock` (not `tokio::sync`) — correct choice for the single-threaded `LocalSet` IPC loop. All lock scopes are synchronous and no `.await` is held across a lock guard.

5. **Error handling**: Production paths use `Result` propagation throughout. `unwrap()` usage is confined to `#[cfg(test)]` blocks (verified via grep). `poisoned` lock handling is graceful in `AgentSlot` (returns `Error` state variant).

6. **JSON-RPC protocol**: Error codes follow the convention (−32601 for method not found, −32700 for parse error, −32000 range for application errors). Notification format correctly omits `id` field.

### Test Quality

1. **`ipc_client_multiplex.rs`**: Excellent concurrent request tests (3 and 10 concurrent), timeout verification, notification handling, and `Send + Sync` assertion.

2. **`multi_agent_worker.rs`**: Comprehensive integration test with a full mock worker server exercising initialize, agent_start/stop/list, concurrent prompt routing, and crash isolation.

3. **`worker_registry.rs`**: Good coverage of capacity limits, removal, shutdown_all, and 16-creator scalability.

4. **`preset_agents.rs`**: Thorough role parsing and recommended_models format validation tests.

---

## Cross-Reviewer Ready Notes

### Integration Risks

- **Session routing correctness**: `InnerGraphNodeTask::resolve_session_id` (tasks/mod.rs:708–729) has a 3-level priority chain that depends on `_session_routes` being set in context by the engine. If the engine fails to populate this key, all routing falls back to `"default"`. This is safe but silent — no warning is logged.

- **Config migration idempotency**: The `config.json → config.toml` migration (config.rs:107–139) is idempotent because it checks for `config.toml` first. However, if migration partially succeeds (TOML written but JSON not renamed), the next load will see both files and prefer the TOML. The orphaned `config.json` would be silently ignored. Acceptable but worth noting.

### Migration Cost

- The old `acp_worker.rs` (single-agent, ~376 lines deleted) has been replaced by the `acp_worker/` module. No references to the old module should remain. Backward compatibility is maintained through the `agent_ref` single-agent path in `handle_initialize`.

---

## Severity Summary

| Severity | Count | Finding IDs |
|----------|-------|-------------|
| Critical | 0 | — |
| Warning | 2 | W1, W2 |
| Suggestion | 4 | S1, S2, S3, S4 |

## Verdict: **Request Changes**

Warnings W1 and W2 must be addressed before this feature can be approved. Both represent functional correctness concerns in production code paths (session management and config resolution). Suggestions S1–S4 are non-blocking but recommended for code quality and test completeness.

---

## Evidence Sources

| Source | Evidence Type |
|--------|--------------|
| `crates/nexus42/src/commands/acp_worker/mod.rs:300-362` | W1: handle_initialize re-init behavior |
| `crates/nexus42/src/config.rs:1171-1173` | W2: resolve_agent_model preset fallback |
| `crates/nexus-orchestration/src/worker/manager.rs:323-332` | S1: duplicated doc comment |
| `crates/nexus-orchestration/src/preset/loader.rs:641` | S2: unwrap_or_default for agent_ref |
| `crates/nexus42/src/commands/acp_worker/mod.rs:776-814` | S3: handle_agent_crash dead_code |
| `crates/nexus-orchestration/src/tasks/mod.rs:956-985` | S4: handle take/return pattern |
| `crates/nexus42/src/commands/acp_worker/mod.rs:863+` | All unwrap() confirmed in #[cfg(test)] |
| `crates/nexus-orchestration/src/worker/transport.rs:269+` | All unwrap() confirmed in #[cfg(test)] |
| `crates/nexus-orchestration/tests/*.rs` | Test coverage review |

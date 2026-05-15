---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-05-15-v1.19-agent-host-hardening"
verdict: "Request Changes"
generated_at: "2026-05-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-05-15

## Scope
- plan_id: `2026-05-15-v1.19-agent-host-hardening`
- Review range / Diff basis: `main...feature/v1.19-agent-host-hardening`
- Working branch (verified): `feature/v1.19-agent-host-hardening`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 18 files, +3000/-183 lines
- Commit range: `main` to `feature/v1.19-agent-host-hardening`

## Tools Run
- `cargo +nightly fmt --all` — passed
- `cargo clippy -p nexus-agent-host --lib -- -D warnings` — passed (lib only; pre-existing test failures in telemetry/events.rs not in scope)
- `cargo test -p nexus-agent-host` — 156 passed
- `cargo test -p nexus-acp-host` — 20 passed
- `cargo test -p nexus-daemon-runtime` — 118 passed

## Findings

### 🔴 Critical

- **F-001: Session state leak when ACP execute() times out**
  - **Location**: `crates/nexus-agent-host/src/providers/acp.rs:510-526`
  - **Source Type**: manual-reasoning
  - **Issue**: When `stream_prompt()` times out in `AcpProvider::execute()`, an error is returned but no `HostEvent::OpFailed` is emitted. The manager's `exec()` path (manager.rs:294) calls `adapter.execute()` directly and returns the stream wrapped in a transition helper (manager.rs:300-315) that only fires on `OpFinished`/`OpFailed` events. If `execute()` returns an error, the stream wrapper never processes anything and the session remains permanently stuck in `Busy` state.
  - **Impact**: After a timeout, the session cannot be reused or properly cleaned up. Calls to `exec()` on that session will fail with "session busy". The ACP session on the SDK side remains orphaned until shutdown.
  - **Fix**: In `AcpProvider::execute()`, when the timeout fires at line 510-523, emit an `OpFailed` event on the error path instead of returning `Err`. Wrap the entire `execute()` body in a stream that first emits `OpStarted`, then either the provider stream or an `OpFailed` on error.
  - **Confidence**: High

### 🟡 Warning

- **F-002: Orphaned ACP session on execute timeout**
  - **Location**: `crates/nexus-agent-host/src/providers/acp.rs:581-593`
  - **Source Type**: manual-reasoning
  - **Issue**: When `execute()` times out, the ACP session (tracked via `acp_session_id` in `AcpSessionState`) remains active on the SDK side. The `shutdown()` method only removes the session from the local `HashMap` — it does NOT cancel the underlying ACP session. The `run_stream_prompt` background task continues until the agent sends `Stopped` or the connection errors.
  - **Impact**: Each timeout leaves an orphaned ACP session consuming resources on the agent side until the agent naturally completes or the connection times out. Accumulated timeouts can leak resources.
  - **Fix**: In `shutdown()`, send a cancel request to the ACP session before removing the local entry, or document that timeouts are expected to self-clean via agent completion.
  - **Confidence**: High

- **F-003: Permission handler registration is racy**
  - **Location**: `crates/nexus-agent-host/src/providers/acp.rs:78-107`
  - **Source Type**: manual-reasoning
  - **Issue**: `AcpProvider::new()` spawns `tokio::spawn(async move { client.set_permission_handler(handler).await })` but does NOT await it. The method returns before the handler is actually registered. If `launch()` or `execute()` is called immediately after construction, the SDK's `on_receive_request` handler will find `permission_handler = None` and cancel permission requests.
  - **Impact**: During the race window between `new()` and handler registration completing, permission requests from the ACP agent are cancelled, potentially blocking agent functionality.
  - **Fix**: Await the handler registration before `new()` returns, or use a `Arc<Notify>` barrier that `execute()` waits on until the handler is confirmed registered.
  - **Confidence**: Medium

### 🟢 Suggestion

- **F-004: HostManager::exec() lacks manager-level timeout guard**
  - **Location**: `crates/nexus-agent-host/src/core/manager.rs:250-315`
  - **Source Type**: manual-reasoning
  - **Issue**: `exec()` calls `adapter.execute()` directly without a manager-level timeout wrapper (manager.rs:294). The timeout enforcement relies entirely on each provider's implementation. If a future provider forgets to add timeout wrapping, operations are unbounded.
  - **Impact**: Future provider implementations that miss timeout wrapping create indefinite-hang risk at the manager layer.
  - **Fix**: Consider adding `tokio::time::timeout()` at the manager level in `exec()` as a defensive fallback, separate from provider-level timeouts. Alternatively, document that all provider implementations MUST timeout their execute path.
  - **Confidence**: Medium

- **F-005: Native CLI shutdown is cosmetic — processes auto-exit**
  - **Location**: `crates/nexus-agent-host/src/providers/native_cli/claude.rs:450-460`
  - **Source Type**: manual-reasoning
  - **Issue**: `ClaudeCliProvider::shutdown()` only removes the session from the local `HashMap`. Since each `execute()` spawns a fresh process (multi-process model, not persistent), there is no child process to kill. The process exits naturally after streaming.
  - **Impact**: No functional issue — this is correct behavior. Noted for documentation completeness.
  - **Confidence**: High

## Source Trace

| Finding | File | Lines | Category |
|---------|------|-------|----------|
| F-001 | `crates/nexus-agent-host/src/providers/acp.rs` | 510-526 | reliability: session state |
| F-002 | `crates/nexus-agent-host/src/providers/acp.rs` | 581-593 | reliability: resource leak |
| F-003 | `crates/nexus-agent-host/src/providers/acp.rs` | 78-107 | reliability: race condition |
| F-004 | `crates/nexus-agent-host/src/core/manager.rs` | 250-315 | reliability: defense-in-depth |
| F-005 | `crates/nexus-agent-host/src/providers/native_cli/claude.rs` | 450-460 | correctness: behavior confirmed |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

### Performance Risk Assessment

| Area | Risk | Notes |
|------|------|-------|
| Regex compilation (D-005) | **Low** | `LazyLock` ensures one-time compilation at first use. Patterns are simple and efficient. |
| Timeout wrapping (D-004) | **Medium** | ACP streaming phase is bounded by the full `stream_prompt` future (resolves on Stopped). Native CLI streaming bounded by process exit. Setup phases are properly timeout-wrapped. |
| Streaming event throughput (D-006) | **Low** | Events flow through mpsc channel with 64-capacity buffer. No unbounded memory growth. |
| Process spawn frequency (D-001) | **Low** | Multi-process model is correct. Session ID injection via `--session-id` avoids output parsing fragility. |

### Reliability Risk Assessment

| Area | Risk | Notes |
|------|------|-------|
| Session state on timeout | **HIGH** | F-001: Session stuck in Busy after timeout |
| Shutdown timeout handling | **Low** | Sequential per-session timeouts work correctly |
| Permission handling | **Medium** | F-003: Race condition in handler setup |
| Error propagation | **Low** | Errors propagate correctly; F-001 is the exception |

### D-004 Timeout Coverage Verification

| Operation | Provider | Timeout Applied To | Coverage |
|-----------|----------|-------------------|----------|
| `probe()` | ACP | `initialize()` call | ✅ Setup phase |
| `probe()` | Native CLI | `which::which()` via `spawn_blocking` | ✅ Lookup phase |
| `launch()` | ACP | `create_session()` call | ✅ Session creation |
| `launch()` | Native CLI | No-op (registration only) | ✅ N/A |
| `execute()` | ACP | `stream_prompt` future (resolves on Stopped) | ✅ Full streaming |
| `execute()` | Native CLI | `spawn_and_write_stdin` (spawn + stdin write) | ⚠️ Setup only; streaming bounded by process exit |
| `shutdown()` | ACP | N/A (synchronous local removal) | ✅ |
| `shutdown()` | Native CLI | N/A (no persistent child) | ✅ |

### Clippy / Format Status

- `cargo +nightly fmt --all` — ✅ Passed
- `cargo clippy --lib` on affected crates — ✅ Passed (pre-existing test failures in `telemetry/events.rs` not in scope)
- `cargo test` — ✅ 294 tests passed across affected crates

**Verdict**: Request Changes

### Reason

F-001 (session state leak on timeout) is a **Critical** reliability defect that causes sessions to become permanently unusable after a timeout. This blocks the core D-004 timeout enforcement from being safe to use in production. F-002 compounds this by leaving orphaned ACP sessions on each timeout.

F-003 (permission handler race) is a **Warning** that can cause permission requests to be incorrectly cancelled during the race window.

Per `mstar-review-qc` gate rules: **Unresolved critical findings => `Request Changes`**.

### Required Before Approve

1. **Fix F-001**: Modify `AcpProvider::execute()` to emit `OpFailed` on the timeout error path so the session state machine transitions back to `Ready`.
2. **Fix F-002**: Add cancel or explicit cleanup for the ACP session when `execute()` times out, or document the self-cleanup expectation.
3. **Fix F-003**: Await permission handler registration before `new()` returns, or add a synchronization barrier.

### Optional Improvements

- F-004: Consider adding manager-level timeout fallback in `exec()` for defense-in-depth.
- F-005: No action needed — behavior is correct.

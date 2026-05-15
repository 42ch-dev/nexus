# Code Review Report

---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-05-15-v1.19-agent-host-hardening"
verdict: "Approve"
generated_at: "2026-05-15"
---

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: glm-5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-05-15T14:30:00Z

## Scope
- plan_id: 2026-05-15-v1.19-agent-host-hardening
- Review range / Diff basis: main...HEAD
- Working branch (verified): feature/v1.19-agent-host-hardening
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 18
- Commit range: main...feature/v1.19-agent-host-hardening (3000+ lines)
- Tools run: git diff, cargo test -p nexus-agent-host, cargo test -p nexus-acp-host, cargo clippy, cargo +nightly fmt --check

## Findings

### 🔴 Critical

(None)

### 🟡 Warning

(None)

### 🟢 Suggestion

| ID | Description | Recommendation |
|----|-------------|----------------|
| S-001 | **Permission handler initialization race**: In `AcpProvider::new()`, the permission handler is set via `tokio::spawn()` which runs asynchronously. While session creation ordering naturally prevents races, the pattern is unconventional and could theoretically allow a prompt to be sent before the handler is registered if callers bypass normal session creation flow. | Consider making `set_permission_handler` blocking in the constructor context (use `block_on` in a spawn_blocking wrapper) or add explicit documentation that prompts must not be sent until the handler is ready. Alternatively, add a readiness signal/flag that `execute()` can check. |
| S-002 | **Shutdown handler still shuts down whole host**: The `shutdown_session` API handler (D-010) validates session ID correctly but still calls `host.shutdown()` for the entire host. The code has a comment noting this Wave 1 limitation, but the pattern is inconsistent with the route name. | When per-session shutdown is implemented in a future wave, ensure the handler returns 404 for valid-but-missing UUIDs as documented. Consider adding a tracking issue. |
| S-003 | **Permission events forwarding task lifecycle**: The `stream_prompt` implementation spawns a task to forward permission events from the SDK's `on_receive_request` handler into the output channel. The task correctly uses `tokio::select!` with a done signal, but the coordination between multiple concurrent components (dispatch loop, permission handler, forwarding task) adds complexity. | Document the channel lifecycle clearly in client.rs comments: who owns which sender/receiver, when each drops, and how the done signal coordinates shutdown. |
| S-004 | **Native CLI shutdown no longer kills child**: After D-001 implementation, `ClaudeCliProvider::shutdown()` simply removes the session from the map without killing a process. This is correct because there is no persistent child (multi-process approach), but the change from Wave 1 (which would kill) could surprise readers expecting process termination. | Add documentation comment explaining that multi-process continuity means no persistent child to kill. The Claude CLI manages its own session persistence via `--resume` flags. |
| S-005 | **AutoToolRiskClassifier default is Write**: Unknown tools default to `Write` risk (not `Read`). This is a conservative security posture but could cause unexpected permission denials for tools that happen to have unfamiliar names. | Document the reasoning: unknown tools are treated as potentially state-mutating because it's safer to ask/deny than to assume read-only. Add a configuration override mechanism if users need to whitelist specific unknown tools as `Read`. |

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| S-001 | git-diff + manual-reasoning | `acp.rs:74-88` — handler set via tokio::spawn | Medium |
| S-002 | git-diff + doc-rule | `agent_host.rs:192-207` — shutdown_session handler | High |
| S-003 | git-diff + manual-reasoning | `client.rs:1363-1411` — permission forwarding task | Medium |
| S-004 | git-diff | `claude.rs:450-453` — shutdown() removes session only | High |
| S-005 | git-diff + manual-reasoning | `risk.rs:133-135` — default fallback to Write | High |

## Architecture Coherence Assessment

### Design Consistency (D-001–D-011)

| Item | Implementation Quality | Notes |
|------|------------------------|-------|
| **D-001** (Native multi-turn) | ✅ Excellent | Session continuity via `--session-id`/`--resume` correctly implemented. Session state tracked in `NativeSession { claude_session_id, first_exec_done }`. Tests verify flag progression. |
| **D-002** (ACP permission) | ✅ Excellent | Permission callback wired through SDK's `on_receive_request` handler. Uses `HostPermissionResolver` + `AutoToolRiskClassifier`. Preference order `AllowAlways > AllowOnce` correct. |
| **D-003** (Capability truthfulness) | ✅ Excellent | `acp_full()` now claims `set_model=false, set_mode=true`. SetMode uses stable `session/set_mode` RPC. SetModel uses dynamic `config_options` discovery. Honest descriptor. |
| **D-004** (Timeout enforcement) | ✅ Excellent | All provider ops wrapped: `probe` → launch_ms, `launch` → launch_ms, `execute` → prompt_ms, `shutdown` → shutdown_ms per-session. Timeout failures produce `OpFailed` with category "timeout". |
| **D-005** (Auto risk classification) | ✅ Excellent | `LazyLock` compiled regex patterns. Precedence: destructive > write > read > default=Write. Static overrides supported. Comprehensive test coverage. |
| **D-006** (Streaming adaptation) | ✅ Good | All `AcpStreamUpdate` variants mapped: `ThoughtDelta`, `ToolCall`, `ToolCallUpdate`, `PlanUpdate`, `PermissionResult`. `session_update_to_acp_updates` handles all SDK variants. |
| **D-007** (Shutdown wiring) | ✅ Excellent | `HostManager::shutdown()` collects sessions → adapters → calls `ProviderAdapter::shutdown()` with per-session timeout → transitions state → clears mappings. Tests verify provider shutdown invoked. |
| **D-008** (Admission policy) | ✅ Excellent | `create_session()` checks provider allow/deny + session limit. `exec()` checks ops-per-session limit. Policy rebuilt from config on `start()`. Custom policy preserved if set via `with_admission()`. |
| **D-009** (Cross-platform probe) | ✅ Excellent | `which` crate used for cross-platform lookup. Wrapped in `spawn_blocking` for async safety. Windows PATHEXT handled in `path_scan.rs`. |
| **D-010** (API validation) | ✅ Good | `parse_session_id()` validates UUID format. Returns 400 Bad Request for malformed. Tests cover invalid/empty/partial cases. |
| **D-011** (Path traversal) | ✅ Excellent | `validate_workspace_path` rejects relative paths and `..` components. `validate_config_path` uses `canonicalize` + prefix check. Tests verify escape rejection. |

### Provider Adapter Interface Contract

- ✅ All adapters implement `ProviderAdapter` trait correctly
- ✅ `shutdown()` contract honored: ACP adapter removes session, native adapter removes from map
- ✅ Timeout wrapping consistent across providers
- ✅ Capability descriptors no longer lie about supported features

### Session State Machine Integrity

- ✅ Transitions: `Created → Ready → Busy → Ready → Stopping → Stopped → Removed`
- ✅ `HostManager::shutdown()` properly transitions through `Stopping → Stopped → Removed`
- ✅ Session registry cleared after all provider shutdowns complete

### Capability Descriptor Honesty (D-003)

- ✅ `acp_full()` claims only what is guaranteed: `set_mode=true` (stable RPC), `set_model=false` (dynamic discovery)
- ✅ `native_cli_limited()` claims `session_restore=true` (via `--resume` support)
- ✅ Test coverage verifies descriptor values match implementation

### Multi-turn Session Continuity (D-001)

- ✅ Claude CLI: host-generated UUID via `--session-id` on first call, `--resume` on subsequent
- ✅ No persistent child process (multi-process approach, correct design)
- ✅ Session state tracked: `claude_session_id`, `first_exec_done`

## Maintainability Assessment

### Code Organization and Module Boundaries

| Area | Rating | Notes |
|------|--------|-------|
| `manager.rs` | ✅ Good | Clear separation: start, create_session, exec, shutdown. Admission policy wiring well-structured. |
| `acp.rs` | ✅ Good | Helper methods for SetMode/SetModel cleanly separated. Stream conversion centralized in `stream_update_to_event`. |
| `claude.rs` | ✅ Good | Session state management isolated. Flag assembly logic clear. `spawn_and_write_stdin` helper reduces duplication. |
| `risk.rs` | ✅ Excellent | Compiled regex via `LazyLock` (no runtime compilation). Trait design extensible. |
| `config.rs` | ✅ Excellent | Path validation helpers clearly documented. Separation of workspace vs config path validation. |
| `client.rs` | ⚠️ Acceptable | `with_connection` is long (~150 lines) but well-commented. Permission handling logic is complex but necessary. |

### Error Handling Patterns

- ✅ Consistent use of `HostError` categories: `policy_denied`, `timeout`, `capability_unsupported`, `launch_failed`
- ✅ Timeout errors include stage name and duration in message
- ✅ Graceful fallback for SetModel: emits `Status` warning + `OpFailed` instead of crashing

### Configuration Injection Patterns

- ✅ `TimeoutConfig` passed to providers at construction
- ✅ `AdmissionPolicy` rebuilt from `AgentHostConfig` on `start()` unless custom
- ✅ `HostPermissionResolver` injected into `AcpProvider::new()`

### Test Coverage Sufficiency

| Module | Tests | Coverage Assessment |
|--------|-------|---------------------|
| `manager.rs` | +9 new tests | ✅ Excellent — covers shutdown wiring, admission limits, policy denial |
| `acp.rs` | +8 new tests | ✅ Excellent — covers stream conversions, permission events |
| `claude.rs` | +3 new tests | ✅ Good — covers session ID generation, flag progression |
| `risk.rs` | +14 new tests | ✅ Excellent — covers all patterns, precedence, overrides |
| `config.rs` | +8 new tests | ✅ Excellent — covers path validation, traversal rejection |
| `client.rs` | +13 new tests | ✅ Excellent — covers permission response building, stream variants |
| `agent_host.rs` | +8 new tests | ✅ Good — covers UUID validation, error responses |

**Total: 156 tests in nexus-agent-host, 157 in nexus-acp-host — all passing**

### Documentation Completeness

- ✅ Module-level docs updated (claude.rs header explains multi-turn model)
- ✅ Function-level docs for new APIs (`set_mode`, `validate_workspace_path`)
- ✅ Capability descriptor docs explain honesty rationale
- ⚠️ Some complex sections (client.rs permission forwarding) could benefit from additional lifecycle comments

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 5 |

**Verdict**: Approve

This is a well-executed hardening release. The implementation correctly addresses all 11 functional items (D-001–D-011) with strong test coverage. Architecture coherence is excellent — capability descriptors no longer lie, timeouts are enforced, admission policy is wired, and multi-turn session continuity works via the documented `--session-id`/`--resume` approach.

The five suggestions are minor improvements focused on documentation and lifecycle clarity, not correctness issues. The permission handler initialization pattern (S-001) is unconventional but correct in practice due to session creation ordering. The other suggestions are documentation opportunities to help future maintainers understand the design decisions.

All QC residual items from V1.18 (R1–R7) are properly addressed by the corresponding D-* implementations, as documented in the compass closure matrix.

**Evidence**:
- `cargo test -p nexus-agent-host`: 156 tests passed
- `cargo test -p nexus-acp-host`: 157 tests passed
- `cargo clippy -p nexus-agent-host -- -D warnings`: clean
- `cargo +nightly fmt --all -- --check`: clean
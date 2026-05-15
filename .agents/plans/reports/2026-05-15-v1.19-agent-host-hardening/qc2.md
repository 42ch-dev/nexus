---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-05-15-v1.19-agent-host-hardening"
verdict: "Request Changes"
generated_at: "2026-05-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: k2p6
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-05-15T20:00:00Z

## Scope
- plan_id: 2026-05-15-v1.19-agent-host-hardening
- Review range / Diff basis: main...feature/v1.19-agent-host-hardening
- Working branch (verified): feature/v1.19-agent-host-hardening
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 18 changed files (3000+ insertions, 183 deletions)
- Commit range: main...HEAD
- Tools run: cargo test -p nexus-agent-host, cargo test -p nexus-acp-host, cargo test -p nexus-daemon-runtime agent_host, cargo clippy --all -- -D warnings

## Findings

### 🔴 Critical

**F-001: Streaming phase lacks timeout enforcement — indefinite hang risk**

- **Issue**: `D-004` (timeout enforcement) adds `tokio::time::timeout` to `probe()`, `launch()`, and `execute()` *setup* phases, but the **streaming consumption phase** in `build_event_stream()` is unbounded. In `claude.rs`, `build_event_stream` reads stdout lines until EOF without any timeout. If the child process hangs and never closes stdout, the stream consumer hangs indefinitely.
- **Affected files**: `crates/nexus-agent-host/src/providers/native_cli/claude.rs:101-165` (build_event_stream), `crates/nexus-agent-host/src/providers/acp.rs` (stream consumption via `stream_update_to_event` loop)
- **Attack vector**: A malicious or buggy provider subprocess that writes partial output and then hangs will block the host session forever, consuming resources and preventing shutdown cleanup.
- **Fix**: Wrap the stream consumption with a per-prompt timeout using `tokio::time::timeout` around the `StreamExt::next()` loop, or add a `max_prompt_duration` that bounds the entire execute operation including streaming.
- **Evidence**: `claude.rs:build_event_stream` has no timeout wrapping; `acp.rs` stream loop reads from unbounded mpsc channel without timeout.

### 🟡 Warning

**F-002: `CreateSessionRequest.cwd` is not validated for path traversal**

- **Issue**: `D-011` validates `workspace_root` in `HostStartConfig` via `validate_workspace_path()`, but `CreateSessionRequest.cwd` (passed as `LaunchSpec.cwd` to provider adapters) receives **no path validation**. For the ACP provider, `spec.cwd` flows directly into `NexusNewSessionRequest::new(spec.cwd)`. While ACP agents are typically sandboxed, a malicious API client could supply a crafted `cwd` to influence agent behavior.
- **Affected files**: `crates/nexus-agent-host/src/core/manager.rs:219-225` (launch_spec construction), `crates/nexus-agent-host/src/providers/acp.rs:421` (acp_request creation)
- **Fix**: Apply `validate_workspace_path()` to `request.cwd` in `create_session()` before constructing `LaunchSpec`, or add a dedicated `validate_cwd()` helper that checks the path is absolute and within a trust boundary.
- **Evidence**: No call to `validate_workspace_path` or `check_workspace_root` on `request.cwd` in `manager.rs:create_session`.

**F-003: TOCTOU in config path validation**

- **Issue**: `manager.rs::start()` checks `config.config_path.exists()` before calling `validate_config_path()`. Between the `exists()` check and `canonicalize()` inside `validate_config_path()`, an attacker could swap the file (race condition). While the risk is low for config *reading*, this pattern violates secure path validation best practices.
- **Affected files**: `crates/nexus-agent-host/src/core/manager.rs:143-146`
- **Fix**: Remove the pre-check and let `validate_config_path()` handle non-existent paths internally. `canonicalize()` will fail for non-existent paths, which is the desired behavior. The outer `exists()` guard is unnecessary.
- **Evidence**: ```rust
if config.config_path.exists() {
    if let Some(expected_dir) = config.config_path.parent() {
        crate::config::validate_config_path(&config.config_path, expected_dir)?;
    }
}
```

**F-004: `shutdown()` constructs `ManagedSessionHandle` with hardcoded `acp_full()` capabilities**

- **Issue**: In `HostManager::shutdown()`, the `ManagedSessionHandle` passed to `ProviderAdapter::shutdown()` always has `capabilities: CapabilityDescriptor::acp_full()`, regardless of the actual provider type or negotiated capabilities. While shutdown doesn't currently use capabilities, this is a latent correctness bug that could cause issues if future shutdown logic depends on capability checks.
- **Affected files**: `crates/nexus-agent-host/src/core/manager.rs:405-409`
- **Fix**: Retrieve the actual negotiated capabilities from the session registry instead of hardcoding `acp_full()`.
- **Evidence**: ```rust
let handle = ManagedSessionHandle {
    provider_id: provider_id.clone(),
    session_id: session_id.clone(),
    capabilities: crate::capability::model::CapabilityDescriptor::acp_full(), // Hardcoded
};
```

**F-005: Permission handler race in `AcpProvider::new()` lacks explicit synchronization guarantee**

- **Issue**: The permission handler is set via `tokio::spawn()` in the constructor. While the default SDK behavior denies if no handler is set (safe), there's no explicit synchronization guaranteeing the handler is installed before the first permission request. Under high concurrency or slow scheduling, a permission request could arrive before `set_permission_handler` completes.
- **Affected files**: `crates/nexus-agent-host/src/providers/acp.rs:108-112`
- **Fix**: Accept an `Option<tokio::sync::Notify>` or return a future from `AcpProvider::new()` that resolves when the handler is set. Alternatively, set the handler synchronously if the SDK adapter supports it.
- **Evidence**: ```rust
tokio::spawn(async move {
    client_for_handler.set_permission_handler(handler).await;
});
```

### 🟢 Suggestion

**S-001: Add timeout to `build_event_stream` for complete D-004 coverage**

- **Issue**: The current timeout implementation covers operation *initiation* but not the full operation lifecycle. For complete `D-004` coverage, consider adding a `session_ms` or `prompt_ms` timeout around the entire `execute()` call including stream consumption.
- **Fix**: Wrap the returned `HostEventStream` with a timeout wrapper, or add a background task that cancels the operation after `prompt_ms`.

**S-002: `validate_workspace_path` should document symlink behavior**

- **Issue**: `canonicalize()` resolves symlinks, which means a symlink to `/etc/passwd` placed in `/tmp/workspace` would be accepted after resolution. This is correct behavior (the resolved path is what's actually used), but it should be documented so callers understand that symlinks are followed.
- **Fix**: Add a doc comment noting that symlinks are resolved and the canonicalized path is returned.

**S-003: Permission outcome mapping loses nuance**

- **Issue**: `PermissionOutcome::Ask` is mapped to `AcpPermissionOutcome::Deny` with a comment that interactive prompting will be added later. This is correct for a non-interactive host, but consider logging at `info` level (not just `warn`) when denying due to "Ask" policy, as this is expected behavior in non-interactive mode.
- **Fix**: Downgrade the log level or add a metric/counter for "ask → deny" decisions.

**S-004: `parse_session_id` test coverage could include edge cases**

- **Issue**: The UUID validation tests don't cover UUIDv7, nil UUID, or max-length edge cases. While `uuid::Uuid::parse_str` handles these correctly, adding explicit test cases would document expected behavior.
- **Fix**: Add tests for `00000000-0000-0000-0000-000000000000`, UUIDv7 strings, and uppercase UUIDs.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| F-001 | manual-reasoning | claude.rs:build_event_stream (no timeout wrapping) | High |
| F-002 | manual-reasoning | manager.rs:219-225 (cwd not validated) | High |
| F-003 | manual-reasoning | manager.rs:143-146 (exists() pre-check) | Medium |
| F-004 | manual-reasoning | manager.rs:405-409 (hardcoded acp_full) | High |
| F-005 | manual-reasoning | acp.rs:108-112 (async spawn in constructor) | Medium |
| S-001 | doc-rule | D-004 compass requirement | High |
| S-002 | manual-reasoning | config.rs:validate_workspace_path | Low |
| S-003 | manual-reasoning | acp.rs:99-101 (Ask→Deny mapping) | Low |
| S-004 | manual-reasoning | agent_host.rs:tests | Low |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 5 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

**Rationale**:

1. **F-001 (Critical)**: The streaming timeout gap is a real availability risk. A hung provider subprocess can block a session indefinitely, violating D-004's "all provider paths must be bounded" requirement and potentially causing resource exhaustion.

2. **F-002 (Warning)**: The unvalidated `cwd` field is a defense-in-depth gap that should be closed before merge, especially since `workspace_root` validation was explicitly added as a security gate.

3. **F-003 (Warning)**: The TOCTOU pattern, while low-severity for config reading, is a code smell that should be cleaned up.

4. **F-004 (Warning)**: Hardcoded capabilities in shutdown could cause subtle bugs in future iterations.

5. **F-005 (Warning)**: The permission handler race is mitigated by safe defaults but should be made explicit.

All other items are suggestions or have safe fallbacks. The test suite passes (156 tests in nexus-agent-host, 8 in nexus-acp-host, 11 in daemon-runtime agent_host), and clippy is clean.

**Blocking issues to resolve before Approve**:
- F-001: Add timeout to streaming phase
- F-002: Validate `CreateSessionRequest.cwd`
- F-003: Remove TOCTOU `exists()` pre-check

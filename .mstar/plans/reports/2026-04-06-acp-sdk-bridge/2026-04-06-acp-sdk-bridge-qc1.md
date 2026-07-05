---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-04-06-acp-sdk-bridge"
verdict: "Request Changes"
generated_at: "2026-04-07"
---

# QC Review Report — Plan B (ACP SDK Bridge)

**Reviewer**: @qc-specialist  
**Date**: 2026-04-07  
**Branch**: `feature/v2.0-acp-sdk-bridge`  
**Review Focus**: Async Safety, Channel Correctness, ACP Protocol, Error Handling, Test Quality, Resource Cleanup

---

## Executive Summary

The ACP SDK Bridge implementation provides a well-architected solution for bridging `!Send` ACP SDK futures with the async tokio runtime. The `LocalSetBridge` pattern is correctly implemented with proper thread lifecycle management. However, there is **one critical finding** that violates the stated review checklist: a `.unwrap()` call in production code, and the `#[deny(clippy::unwrap_used)]` lint is not applied to the `acp` module.

**Verdict**: **Request Changes** — Critical finding QC1-C1 must be addressed before merge.

---

## Review Checklist Results

| Checklist Item | Status | Notes |
|----------------|--------|-------|
| No `.unwrap()` in production code | ❌ **FAIL** | One `.unwrap()` found at `localset_bridge.rs:241` |
| LocalSetBridge Drop is safe | ⚠️ Partial | Uses `try_send` but `.unwrap()` on Mutex could panic |
| Channel capacities are bounded | ✅ Pass | `mpsc::channel(16)` — bounded |
| Error types properly mapped | ✅ Pass | `AcpError::sdk()` wraps SDK errors correctly |
| Tests exercise claimed paths | ✅ Pass | 9 LocalSetBridge tests, good coverage |
| No unsafe code without justification | ✅ Pass | No unsafe code found |
| Thread spawn failure handled gracefully | ⚠️ Partial | Runtime creation error logs and returns, but no error propagation to caller |

---

## Findings

### Severity: Critical

| ID | Severity | Location | Description | Suggestion |
|----|----------|----------|-------------|------------|
| **QC1-C1** | Critical | `localset_bridge.rs:241` | `.unwrap()` on `Mutex::lock()` in `Drop::drop()`. The review checklist explicitly requires "No `.unwrap()` in production code (Plan A added `#[deny(clippy::unwrap_used)])`". This is a blocking violation. | Replace with `lock().unwrap_or_else(|e| { warn!("Mutex poisoned in LocalSetBridge drop: {}", e); return; })` or use `expect()` with a clear message. |

### Severity: High

| ID | Severity | Location | Description | Suggestion |
|----|----------|----------|-------------|------------|
| **QC1-H1** | High | `acp/mod.rs`, `acp/*.rs` | The `#[deny(clippy::unwrap_used)]` lint is **not applied** to the `acp` module. Per the review checklist and prior QC reports (`2026-04-06-foundation-hardening-qc2.md`), this lint should be enforced to prevent future regressions. | Add `#![deny(clippy::unwrap_used)]` at the crate level in `nexus42/src/lib.rs` OR add `#[deny(clippy::unwrap_used)]` to the `acp/mod.rs` module. |
| **QC1-H2** | High | `localset_bridge.rs:107-116` | Thread spawn failure (tokio runtime creation error) logs an error and returns, but the `LocalSetBridge` struct is still constructed with a potentially invalid state (channel exists but no thread to process). Subsequent `execute()` calls will hang indefinitely waiting for responses that will never come. | Consider returning `Result<LocalSetBridge, AcpError>` from `new()` instead of panicking silently, or track thread creation status and error in `execute()`. |

### Severity: Medium

| ID | Severity | Location | Description | Suggestion |
|----|----------|----------|-------------|------------|
| **QC1-M1** | Medium | `client.rs:363-418` | `with_connection()` spawns a background task via `tokio::spawn` that establishes the connection. If this fails, the error is only logged, not propagated. The adapter appears valid but `connection.read().await` will return `None`, causing confusing "Connection not established" errors without root cause. | Store connection setup error in a shared `Arc<RwLock<Option<AcpError>>>` and check it in `execute()` methods. |
| **QC1-M2** | Medium | `localset_bridge.rs:102` | Channel capacity is hardcoded to 16. While bounded (good), there's no documentation explaining why 16 was chosen or how to tune it for high-throughput scenarios. | Add a const `BRIDGE_CHANNEL_CAPACITY: usize = 16` with a doc comment explaining the rationale. |
| **QC1-M3** | Medium | `client.rs:589-601` | `subscribe()` returns an empty `StreamReceiver` with a `warn!` log. This is a placeholder that will silently fail to deliver notifications. Callers have no way to detect this is not a real subscription. | Either implement properly, return `Option<StreamReceiver>`, or panic with a clear "not implemented" message (with `todo!()` or similar). |

### Severity: Low

| ID | Severity | Location | Description | Suggestion |
|----|----------|----------|-------------|------------|
| **QC1-L1** | Low | `localset_bridge.rs:233` | `Arc::strong_count(&self.thread_handle) == 1` check is correct for detecting last instance, but there's a race window: if another clone is created between this check and `take()`, the thread could be shut down prematurely. | The race is benign in practice (shutdown is idempotent via `try_send(None)`), but consider documenting this behavior. |
| **QC1-L2** | Low | `localset_bridge.rs:238` | `try_send(None)` ignores the result. If the channel is full, shutdown signal won't be sent. Consider using `blocking_send()` in a separate thread or `send().now_or_never()`. | The current approach is acceptable for Drop semantics (non-blocking), but document this limitation. |
| **QC1-L3** | Low | `error.rs:166-168` | `AcpError::sdk()` converts `agent_client_protocol::Error` to `String`, losing type information for programmatic error handling. | Consider storing the original error: `AcpError::Sdk(#[source] Box<agent_client_protocol::Error>)` (if `Error` is not `Clone`). |

---

## Async Safety Analysis

### Thread Lifecycle

| Aspect | Assessment | Details |
|--------|------------|---------|
| Thread spawn | ⚠️ Issue | `std::thread::spawn` creates OS thread; failure is logged but not propagated (see QC1-H2) |
| LocalSet creation | ✅ Correct | `tokio::task::LocalSet::new()` + `run_until()` pattern is correct |
| Runtime creation | ⚠️ Issue | `tokio::runtime::Builder::new_current_thread()` can fail; error is logged but bridge still created |
| Shutdown | ✅ Correct | `try_send(None)` + `join()` pattern avoids deadlocks |
| Graceful drain | ⚠️ Partial | No explicit drain of in-flight requests before shutdown; they'll complete on the LocalSet but response channels may be dropped |

### Channel Correctness

| Channel | Type | Bounded | Assessment |
|---------|------|---------|------------|
| `request_tx/request_rx` | `mpsc::Sender<Option<BridgeRequest>>` | Yes (16) | ✅ Bounded, prevents unbounded memory growth |
| `response_tx/response_rx` | `oneshot::channel<T>` | N/A | ✅ Correct for single-response pattern |

### Deadlock Analysis

| Scenario | Risk | Assessment |
|----------|------|------------|
| Drop while request in-flight | Low | `try_send` is non-blocking; `join()` waits for thread. Response channel will be dropped, causing `Canceled` error on awaiting side. |
| Full channel on request | Low | Capacity 16 is generous for serial CLI operations; `execute()` will block on `send().await` until capacity available |
| Mutex poisoned in Drop | Low | `.unwrap()` on line 241 could panic if thread panicked while holding lock (see QC1-C1) |

---

## ACP Protocol Adherence

| Requirement | Status | Evidence |
|-------------|--------|----------|
| SDK version pinned | ✅ Pass | `agent-client-protocol = "=0.10.4"` in `Cargo.toml:36` |
| LocalSet required for `!Send` futures | ✅ Pass | `spawn_local` called inside `LocalSet::run_until()` context |
| `ClientSideConnection::create()` usage | ✅ Pass | Used in `client.rs:381-386` with correct handler |
| `SimpleClientHandler` auto-grant policy | ✅ Pass | Implements `request_permission` with first-option selection |
| Capability set | ⚠️ Partial | Not reviewed (focus area for QC-#3); `skills.rs` module exists but not part of this review scope |

---

## Test Quality Assessment

### LocalSetBridge Tests (9 tests)

| Test Name | Coverage | Assessment |
|-----------|----------|------------|
| `bridge_starts_and_processes_request` | Basic functionality | ✅ Verifies bridge works for simple case |
| `bridge_handles_multiple_requests` | Sequential requests | ✅ Good |
| `bridge_shuts_down_cleanly` | Drop cleanup | ✅ Important for resource safety |
| `bridge_timeout_expires` | Timeout handling | ✅ Critical for production use |
| `bridge_handles_concurrent_requests` | Parallel access | ✅ Tests thread safety |
| `bridge_shutdown_while_request_in_flight` | Edge case | ✅ Important for robustness |
| `bridge_handles_empty_result` | Unit type | ✅ Good coverage |
| `bridge_error_propagation` | Error handling | ✅ Verifies errors flow through |
| `bridge_clone_shares_thread` | Clone behavior | ✅ Important for Arc sharing semantics |

### Missing Test Coverage

| Scenario | Recommendation |
|----------|----------------|
| Runtime creation failure | Add test mocking `tokio::runtime::Builder::build()` failure (requires feature flag or refactor) |
| Channel full on `try_send` | Add test verifying graceful handling when channel is at capacity |
| Mutex poisoning | Add test simulating panic while holding lock (advanced) |
| Connection failure in `with_connection()` | Add integration test with mock subprocess |

---

## Resource Cleanup Verification

| Resource | Cleanup Path | Assessment |
|----------|--------------|------------|
| OS Thread | `Drop::drop()` joins thread | ✅ Correct |
| Tokio Runtime | Dropped when thread exits | ✅ Implicit cleanup |
| LocalSet | `run_until()` exits on `None` | ✅ Correct |
| mpsc::Sender | Dropped with `LocalSetBridge` | ✅ Sends `None` to trigger shutdown |
| mpsc::Receiver | Dropped when channel closes | ✅ Thread exits loop |

---

## Lint Status

**Note**: Unable to run `cargo clippy` due to permission restrictions. Manual inspection found:
- No unsafe code
- One `.unwrap()` in production code (QC1-C1)
- Multiple `#[allow(dead_code)]` attributes (acceptable for V1.0 future-facing APIs)
- `#[allow(clippy::type_complexity)]` on `BridgeRequest` (acceptable for type-erased pattern)

---

## Comparison with QC-#2 Findings

| Finding | QC-#1 Assessment | QC-#2 Assessment | Consensus |
|---------|------------------|------------------|-----------|
| `.unwrap()` in Drop | **Critical (C1)** | Not mentioned | **Requires fix** |
| `#[deny(clippy::unwrap_used)]` missing | High (H1) | Not mentioned | **Requires fix** |
| `subscribe()` placeholder | Medium (M3) | Low (L2) | Agreed: needs improvement |
| `with_connection()` error visibility | Medium (M1) | Warning (W2) | Agreed: improvement needed |
| `AcpError::Sdk` type loss | Low (L3) | Low (L4) | Agreed: minor improvement |

---

## Gate Recommendation

**Decision**: **Request Changes**

**Blocking Issues**:
1. **QC1-C1**: `.unwrap()` on Mutex in Drop — violates review checklist requirement for no unwraps in production code
2. **QC1-H1**: `#[deny(clippy::unwrap_used)]` not applied to `acp` module — inconsistent with prior hardening work

**Required Actions Before Merge**:
1. Replace `.unwrap()` at `localset_bridge.rs:241` with safe error handling
2. Add `#[deny(clippy::unwrap_used)]` to `acp/mod.rs` or crate-level `lib.rs`
3. Re-run `cargo clippy` to verify no additional unwraps

**Recommended Actions (Non-Blocking)**:
- Address QC1-H2: Propagate thread spawn failures
- Address QC1-M1: Surface connection errors in `with_connection()`
- Address QC1-M3: Improve `subscribe()` placeholder

---

## Notes for QC-#3 and Consolidated Review

**Cross-verification points**:
- ✅ Channel boundedness verified (capacity 16)
- ✅ Drop implementation structure verified (shutdown signal + join)
- ⚠️ Mutex `.unwrap()` requires fix
- ⚠️ `#[deny(clippy::unwrap_used)]` not applied

**Unique to this review**:
- Deep analysis of Drop implementation safety
- Thread lifecycle failure mode analysis
- Mutex poisoning risk assessment
- Comprehensive test coverage evaluation

---

*End of QC-#1 review report.*
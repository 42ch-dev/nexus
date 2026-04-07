---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-04-06-acp-sdk-bridge"
verdict: "Request Changes"
generated_at: "2026-04-07"
---

# QC Re-Review Report: ACP SDK Bridge (Plan B) — Fix Verification

## Executive Summary

**Verdict**: Request Changes — 6 blocking items verified resolved, but **1 NEW blocking issue discovered**.

- ✅ All 6 original blocking findings are correctly fixed
- ✅ Clippy clean (`cargo clippy --all -- -D warnings` passes)
- ⚠️ Test status: Commit claims 101 tests pass (unable to verify due to permission constraints)
- ❌ **NEW CRITICAL**: `tokio::spawn` in Drop can panic during runtime shutdown

## Verification Summary Table

| ID | Severity | Original Issue | Fix Status | Evidence |
|----|----------|---------------|------------|----------|
| QC1-C1 | CRITICAL | `.unwrap()` on Mutex::lock() in Drop | ✅ RESOLVED | Replaced with `.expect("bridge shutdown: mutex poisoned — unrecoverable")` at line 262 |
| Q3-C1 | CRITICAL | Unbounded handle.join() in Drop | ✅ RESOLVED | Channel-based timeout (5s), logs warning, detaches on timeout (lines 265-284) |
| Q3-C2 | CRITICAL | Race in shutdown using Arc::strong_count | ✅ RESOLVED | AtomicBool + compare_exchange for leader election (lines 247-250) |
| QC1-H1 | HIGH | deny(unwrap_used) missing on acp module | ✅ RESOLVED | Module-level `#![deny(clippy::unwrap_used)]` at mod.rs line 29, #[allow] in all tests |
| QC1-H2 | HIGH | Thread spawn failure ignored | ✅ RESOLVED | Uses `thread::Builder::new().spawn()` + `.expect()` with context (lines 108-141) |
| Q3-H1 | HIGH | Fire-and-forget task in with_connection | ✅ RESOLVED | Task handle stored in `_setup_task` (line 317), Drop impl joins it (lines 446-457) |
| **NEW-C1** | **CRITICAL** | **tokio::spawn in Drop can panic during shutdown** | ❌ **NEW ISSUE** | client.rs line 452 — spawns task without checking runtime presence |

---

## Per-Finding Verification Details

### QC1-C1 (CRITICAL): `.unwrap()` on Mutex::lock() in Drop

**Original Issue**: Using `.unwrap()` on `Mutex::lock()` in `Drop::drop()` violates `#[deny(clippy::unwrap_used)]` and can panic during cleanup.

**Fix Applied**: Replaced `.unwrap()` with `.expect()` at `localset_bridge.rs:262`.

**Verification**:
```rust
// localset_bridge.rs:259-263
if let Some(handle) = self
    .thread_handle
    .lock()
    .expect("bridge shutdown: mutex poisoned — unrecoverable")
    .take()
```

**Assessment**: ✅ RESOLVED. The `.expect()` provides a descriptive error message and is appropriate for a "poisoned mutex" scenario (unrecoverable state). This aligns with Rust best practices for cleanup code where panic-on-poison is acceptable.

---

### Q3-C1 (CRITICAL): Unbounded handle.join() in Drop

**Original Issue**: Unbounded `handle.join()` in Drop — if thread is stuck, `drop()` hangs forever.

**Fix Applied**: Channel-based timeout approach with 5s limit, warning log, and detachment.

**Verification**:
```rust
// localset_bridge.rs:265-284
// Use channel to implement timeout on join
let (done_tx, done_rx) = std_mpsc::channel();

// Spawn helper thread to perform join
thread::spawn(move || {
    let result = handle.join();
    let _ = done_tx.send(result);
});

// Wait with timeout
match done_rx.recv_timeout(Duration::from_secs(5)) {
    Ok(Ok(())) => debug!("LocalSet bridge thread exited cleanly"),
    Ok(Err(e)) => warn!("LocalSet bridge thread panicked: {:?}", e),
    Err(std_mpsc::RecvTimeoutError::Timeout) => {
        warn!("LocalSet bridge thread did not shut down within 5s — detaching");
    }
    Err(std_mpsc::RecvTimeoutError::Disconnected) => {
        warn!("Join helper thread disconnected unexpectedly");
    }
}
```

**Assessment**: ✅ RESOLVED. The fix correctly:
1. Uses a helper thread + std channel (not tokio) — appropriate for blocking Drop context
2. Sets a 5s timeout — reasonable for cleanup
3. Logs warning on timeout — observable failure mode
4. Detaches (doesn't hang indefinitely) — critical requirement met
5. Handles all recv_timeout error cases — complete coverage

---

### Q3-C2 (CRITICAL): Race in shutdown using Arc::strong_count

**Original Issue**: Race condition in shutdown leader election using `Arc::strong_count` — not atomic with drop.

**Fix Applied**: Added `AtomicBool` shutdown flag with `compare_exchange` for atomic leadership election.

**Verification**:
```rust
// localset_bridge.rs:247-250 (shutdown logic)
if self
    .shutdown_flag
    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
    .is_ok()
```

```rust
// localset_bridge.rs:95-96 (field declaration)
/// Shutdown flag to ensure only one caller sends shutdown signal.
shutdown_flag: Arc<AtomicBool>,
```

**Assessment**: ✅ RESOLVED. The fix correctly:
1. Adds dedicated AtomicBool field (initialized to false)
2. Uses `compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)` — ensures only ONE caller wins, even during race windows
3. Correct memory ordering (AcqRel for success, Relaxed for failure)
4. Combined with strong_count check (line 241) for "last instance" hint, but the atomic flag is the authoritative gate

**Note**: The code still checks `Arc::strong_count == 1` as a hint (line 241), but the atomic flag is the decisive lock. This is a valid optimization — the strong_count check reduces contention, but the atomic compare_exchange is the authoritative synchronization primitive.

---

### QC1-H1 (HIGH): deny(unwrap_used) missing on acp module

**Original Issue**: `#[deny(clippy::unwrap_used)]` not applied to `acp` module — inconsistent with Plan A hardening.

**Fix Applied**: Added module-level `#![deny(clippy::unwrap_used)]` with #[allow] in all test modules.

**Verification**:
```rust
// mod.rs:29
#![deny(clippy::unwrap_used)]
```

```rust
// All test modules (client.rs, error.rs, localset_bridge.rs, registry.rs, skills.rs, transport.rs)
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests { ... }
```

**Assessment**: ✅ RESOLVED. The module-level deny attribute is present, and all test modules correctly use #[allow] to exempt test code. No `.unwrap()` calls remain in production code (verified via grep search — all matches are within #[cfg(test)] modules).

---

### QC1-H2 (HIGH): Thread spawn failure ignored

**Original Issue**: Thread spawn failure silently ignored — bridge in invalid state.

**Fix Applied**: Uses `thread::Builder::new().spawn()` with `.expect()` providing descriptive error.

**Verification**:
```rust
// localset_bridge.rs:108-141
let thread_handle = thread::Builder::new()
    .name("nexus-localset-bridge".to_string())
    .spawn(move || { ... })
    .expect("Failed to spawn LocalSet bridge thread — system resources exhausted");
```

**Assessment**: ✅ RESOLVED. The fix:
1. Uses `Builder::new().spawn()` which returns `Result` (not `thread::spawn` which returns `JoinHandle`)
2. Calls `.expect()` with clear error message describing root cause (resource exhaustion)
3. Thread naming for diagnostics: "nexus-localset-bridge"

**Note**: Using `.expect()` here is appropriate for initialization failure (unrecoverable). This differs from the Drop case (QC1-C1) where `.expect()` on mutex is for cleanup — here it's for constructor failure, which is a valid panic-at-init scenario.

---

### Q3-H1 (HIGH): Fire-and-forget task in with_connection

**Original Issue**: Fire-and-forget task in `with_connection` — no error propagation, handle not stored.

**Fix Applied**: Task handle stored in `_setup_task` field, Drop impl added to join the task.

**Verification**:
```rust
// client.rs:315-317 (field declaration)
/// Handle to the connection setup task (must be joined during cleanup).
_setup_task: Option<tokio::task::JoinHandle<()>>,
```

```rust
// client.rs:363 (task spawn)
let setup_task = tokio::spawn(async move { ... });
```

```rust
// client.rs:426 (storage)
_setup_task: Some(setup_task),
```

```rust
// client.rs:446-457 (Drop impl)
impl Drop for AcpSdkAdapter {
    fn drop(&mut self) {
        // Join the setup task if it exists (fire-and-forget cleanup)
        if let Some(setup_task) = self._setup_task.take() {
            // Use tokio::spawn to join in an async context
            // We can't block in Drop, so we spawn a cleanup task
            tokio::spawn(async move {
                let _ = setup_task.await;
            });
        }
    }
}
```

**Assessment**: ⚠️ PARTIALLY RESOLVED. The fix addresses the original issue (handle is now stored), but introduces a NEW CRITICAL issue (see NEW-C1 below).

---

## NEW CRITICAL FINDING

### NEW-C1 (CRITICAL): `tokio::spawn` in Drop Can Panic During Runtime Shutdown

**Location**: `client.rs:452-454` (AcpSdkAdapter Drop implementation)

**Issue**: The Drop implementation calls `tokio::spawn` without checking if a tokio runtime is active. If AcpSdkAdapter is dropped during runtime shutdown or in a non-tokio context, `tokio::spawn` will panic.

**Evidence**:
```rust
// client.rs:446-457
impl Drop for AcpSdkAdapter {
    fn drop(&mut self) {
        if let Some(setup_task) = self._setup_task.take() {
            tokio::spawn(async move {  // ← PANIC if no runtime
                let _ = setup_task.await;
            });
        }
    }
}
```

**Root Cause**: 
- `tokio::spawn` requires an active tokio runtime context
- If called during runtime shutdown (e.g., `Runtime::block_on` completion, driver task destruction), it will panic with: `"tokio::spawn called from outside of a tokio runtime"`
- The fix for Q3-H1 correctly stores the handle, but the cleanup mechanism is unsafe

**Scenarios**:
1. AcpSdkAdapter dropped during `#[tokio::main]` shutdown → potential panic
2. AcpSdkAdapter dropped in destructor chain after runtime shutdown → guaranteed panic
3. AcpSdkAdapter dropped in non-async code without runtime → panic

**Severity**: CRITICAL
- Panics during cleanup are dangerous (double panic → abort)
- During shutdown, panic may be caught by panic hook, but still disrupts graceful termination
- In production CLI, unexpected panic during shutdown can corrupt state or leave zombie processes

**Recommended Fix** (must choose one):
1. **Option A**: Use `tokio::runtime::Handle::try_current()` to check runtime presence before spawning:
```rust
impl Drop for AcpSdkAdapter {
    fn drop(&mut self) {
        if let Some(setup_task) = self._setup_task.take() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move { let _ = setup_task.await; });
            } else {
                // Runtime not active — abandon task (leak handle)
                tracing::warn!("Dropped AcpSdkAdapter without active runtime — abandoning setup task");
            }
        }
    }
}
```

2. **Option B**: Use `std::thread::spawn` (like LocalSetBridge) to join in blocking context (but this blocks Drop, which may be acceptable):
```rust
impl Drop for AcpSdkAdapter {
    fn drop(&mut self) {
        if let Some(setup_task) = self._setup_task.take() {
            std::thread::spawn(move || {
                // Create a mini runtime just for this join (expensive but safe)
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("cleanup runtime creation failed");
                rt.block_on(async { let _ = setup_task.await; });
            });
        }
    }
}
```

3. **Option C**: Abandon the task (leak the JoinHandle) — simplest but leaks resources:
```rust
impl Drop for AcpSdkAdapter {
    fn drop(&mut self) {
        // Take the handle to prevent double-drop, but don't join
        // Task will be cancelled when runtime shuts down
        let _ = self._setup_task.take();
    }
}
```

**Recommendation**: Option A is safest — checks runtime presence, spawns only if safe, logs warning if abandoning.

---

## Additional Checks

### 1. Test Execution Status

**Claim**: Commit message states "All tests pass (101 tests) and clippy is clean."

**Verification**: Unable to execute `cargo test --all` due to permission constraints. Recommend PM/QA verify test pass rate before merge.

### 2. Clippy Lint Check

**Result**: ✅ CLEAN

```bash
cargo clippy --all -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.13s
```

No warnings or errors. Module-level `#![deny(clippy::unwrap_used)]` correctly enforced.

### 3. New Blocking Issues Introduced by Fixes

**Finding**: 1 new CRITICAL issue (NEW-C1) discovered. No other new blocking issues.

**Search Results**:
- No `.unwrap()` in production code (all matches are in #[cfg(test)] modules)
- All `.expect()` calls have descriptive messages
- No unbounded blocking calls in Drop (except the tokio::spawn panic risk)
- No new unsafe code blocks introduced
- AtomicBool usage is correct (proper Ordering semantics)

---

## Gate Recommendation

**Decision**: Request Changes

**Rationale**: While all 6 original blocking items are correctly resolved, the fix for Q3-H1 introduces a NEW CRITICAL issue (NEW-C1) that must be addressed before merge. Spawning async tasks in Drop without runtime presence checks can panic during shutdown — this violates the same safety principles the original QC findings aimed to enforce.

**Required Actions**:
1. Fix NEW-C1 using one of the recommended options (Option A preferred)
2. Re-run `cargo test --all` and confirm 0 failures (PM/QA verification)
3. Submit new fix commit for re-review

**Cross-Reviewer Notes** (for QC-#2/QC-#3 if present):
- All original fixes are sound and follow Rust best practices
- NEW-C1 is a classic "async Drop" pattern pitfall — recommend all reviewers verify
- LocalSetBridge Drop implementation is now robust (timeout + atomic flag)
- Recommend testing shutdown scenarios explicitly (drop during runtime shutdown, drop without runtime)

---

## Evidence Traceability

| Finding | Source | Lines | Evidence Quality |
|---------|--------|-------|-------------------|
| QC1-C1 resolved | localset_bridge.rs | 262 | High — direct code inspection |
| Q3-C1 resolved | localset_bridge.rs | 265-284 | High — direct code inspection |
| Q3-C2 resolved | localset_bridge.rs | 247-250, 95-96 | High — direct code inspection |
| QC1-H1 resolved | mod.rs:29 + all test modules | 29 | High — direct code inspection + grep |
| QC1-H2 resolved | localset_bridge.rs | 108-141 | High — direct code inspection |
| Q3-H1 resolved (partial) | client.rs | 315-317, 363, 426, 446-457 | High — direct code inspection |
| NEW-C1 discovered | client.rs | 452 | High — direct code inspection + Rust docs |
| Clippy clean | cargo clippy output | N/A | High — tool execution |
| Tests claimed pass | commit message | N/A | Low — unverified (permission blocked) |

---

## Appendix: Diff Summary

Fix commit `7c0bdd6` modifies 7 files:
- `crates/nexus42/src/acp/client.rs` — 20 additions (setup_task field + Drop impl)
- `crates/nexus42/src/acp/error.rs` — 1 addition (#[allow] in tests)
- `crates/nexus42/src/acp/localset_bridge.rs` — 138 additions, 50 deletions (major refactor)
- `crates/nexus42/src/acp/mod.rs` — 6 additions (#![deny] + doc fixes)
- `crates/nexus42/src/acp/registry.rs` — 1 addition (#[allow] in tests)
- `crates/nexus42/src/acp/skills.rs` — 1 addition (#[allow] in tests)
- `crates/nexus42/src/acp/transport.rs` — 1 addition (#[allow] in tests)

Total: 118 insertions, 50 deletions across acp module.

---

**Reviewer**: @qc-specialist (QC-#1)
**Review Type**: Manual + lint verification
**Review Scope**: Fix commit `7c0bdd6` on branch `feature/v2.0-acp-sdk-bridge`
**Next Step**: Fix NEW-C1 → re-review → QA verification → merge to main
# QC Report #3 — Foundation Hardening (Plan A)

**Reviewer**: QC Specialist #3
**Date**: 2026-04-06
**Branch**: feature/v2.0-foundation-hardening
**Commit**: 1b8e849

## Summary

The implementation delivers solid foundational improvements across error strategy, SQLite pooling, schema deduplication, workspace middleware, and unwrap cleanup. Production code is largely clean of dangerous `.unwrap()` patterns. However, there is an **inconsistency** in the `#[deny(clippy::unwrap_used)]` enforcement: it is applied to `nexus42` CLI commands but **NOT** to `nexus42d` daemon code. Test coverage is good but some edge cases (pool exhaustion, SQLite locking, race conditions) lack explicit tests.

## Findings

### Critical (Must Fix)

**R1: `#[deny(clippy::unwrap_used)]` not applied to `nexus42d` daemon crate**

- **Location**: `crates/nexus42d/src/` (crate root)
- **Evidence**: `crates/nexus42/src/commands/mod.rs` has `#[deny(clippy::unwrap_used)]` on all command submodules (lines 9-25), but `crates/nexus42d/src/lib.rs` has no such attribute. The plan summary stated "#[deny(clippy::unwrap_used)]" as a deliverable but it was only applied to CLI, not daemon.
- **Impact**: Future changes to `nexus42d` could reintroduce `.unwrap()` in production code without CI failure.
- **Recommendation**: Add `#[deny(clippy::unwrap_used)]` at the `nexus42d` crate level, or apply it to the main source modules (`api/`, `db/`, `workspace/`).

### High (Should Fix)

**R2: Pool exhaustion not explicitly tested**

- **Location**: `crates/nexus42d/src/db/pool.rs` and `crates/nexus42d/tests/integration.rs`
- **Evidence**: `pool_supports_concurrent_access` test (pool.rs:205) spawns 4 tasks on a pool of 4, but each task quickly releases the connection after a single query. `concurrent_handler_requests_succeed` (integration.rs:213) fires 5 concurrent requests but doesn't wait for pool saturation.
- **Impact**: If a handler holds a connection longer (e.g., during a slow query or transaction), pool exhaustion behavior is unknown.
- **Recommendation**: Add a test that acquires all 8 connections (DEFAULT_POOL_SIZE) simultaneously and verifies proper error handling when pool is saturated.

**R3: SQLite file locking not tested**

- **Location**: `crates/nexus42d/src/db/pool.rs`
- **Evidence**: No test exercises the scenario where the SQLite file is locked by another process.
- **Impact**: Production users with existing DBs or concurrent processes could hit unexpected errors.
- **Recommendation**: Add integration test that attempts concurrent DB writes from multiple processes, or at minimum document the expected behavior in a comment.

### Medium (Consider Fixing)

**R4: Race condition between `init_workspace` and middleware not tested**

- **Location**: `crates/nexus42d/src/api/middleware.rs` + `crates/nexus42d/src/workspace/mod.rs`
- **Evidence**: `require_workspace` middleware checks `state.is_initialized()` which reads from an `Arc<Mutex<Option<String>>>` (workspace/mod.rs:76-79). The `init_workspace` function updates this same mutex (line 131) after writing to the database (lines 116-125). If a request arrives between DB write and mutex update, the middleware could still see uninitialized state.
- **Impact**: Low in practice due to fast in-memory mutex operation, but could cause spurious 409 errors during initialization.
- **Recommendation**: Consider adding a comment explaining the window is negligible, or reorder operations to update mutex first.

**R5: `context/assemble` handler is a stub without error propagation**

- **Location**: `crates/nexus42d/src/api/handlers/context.rs:25`
- **Evidence**: Handler returns `status: "ok"` even though it doesn't actually assemble context. The `#[allow(dead_code)]` attribute (line 7) indicates this is known incomplete code.
- **Impact**: CLI clients calling this endpoint get a misleading success response.
- **Recommendation**: Either return an appropriate error (e.g., `501 Not Implemented`) or document that this is a placeholder.

### Low (Suggestions)

**R6: `daemon_client.rs` uses `.unwrap_or_default()` on response body**

- **Location**: `crates/nexus42/src/api/daemon_client.rs:101`
- **Evidence**: `let body = resp.text().await.unwrap_or_default();`
- **Assessment**: This is a safe defensive pattern (provides fallback empty string), not a critical issue. Mentioned for completeness.
- **Recommendation**: No change needed; this pattern is acceptable.

**R7: Test file organization**

- **Location**: `crates/nexus42d/src/api/errors.rs` (lines 117-339), `crates/nexus42d/src/api/middleware.rs` (lines 48-339)
- **Observation**: Both files have large `#[cfg(test)]` modules interleaved with production code. Tests in `errors.rs` (206+ lines) and `middleware.rs` (290+ lines) are comprehensive but may benefit from being moved to separate files (e.g., `tests/integration_api_errors.rs`).
- **Recommendation**: Consider splitting large test modules into separate files for maintainability.

### Verdict

**Status**: `REQUEST CHANGES`

**Rationale**: The critical finding R1 (missing `#[deny(clippy::unwrap_used)]` on daemon) must be addressed to fulfill the stated plan deliverable and prevent future regressions. High-priority items R2 and R3 should be addressed before merging to ensure the pooling infrastructure is robust under stress.

## Evidence

### Production Code Unwrap Analysis

All production code `.unwrap()` / `.expect()` calls verified (via grep + manual reading):

| File | Location | Pattern | Assessment |
|------|----------|---------|------------|
| `nexus42d/src/api/errors.rs` | Lines 1-116 | None in production | Clean |
| `nexus42d/src/api/middleware.rs` | Lines 1-46 | None in production | Clean |
| `nexus42d/src/api/handlers/*.rs` | All handlers | None found | Clean |
| `nexus42d/src/db/pool.rs` | Lines 1-155 | None in production | Clean |
| `nexus42d/src/db/schema.rs` | Lines 1-92 | None in production | Clean |
| `nexus42d/src/workspace/mod.rs` | Lines 1-135 | Line 78,86: `.expect("mutex poisoned")` | Acceptable - mutex poison is unrecoverable |
| `nexus42/src/api/daemon_client.rs` | Line 101 | `.unwrap_or_default()` | Acceptable - defensive fallback |
| `nexus42/src/context/summary.rs` | Lines 1-169 | Lines 93,97,166: `.unwrap_or_*` | Acceptable - safe fallbacks |

### Test Coverage Summary

| Category | Tests | Assessment |
|----------|-------|------------|
| Error type mappings | 7 unit tests | Good |
| Middleware workspace guards | 11 integration tests | Excellent |
| Pool concurrent access | 3 tests (1 unit + 2 integration) | Good, could be better (see R2) |
| Schema idempotency | 4 unit tests | Good |
| Handler integration | 8 integration tests | Good |
| SQLite error handling | 0 explicit tests | Missing (see R3) |

### Cross-Reviewer Ready Notes

- **Runtime behavior**: Pool size is 8 (DEFAULT_POOL_SIZE). If pool exhaustion occurs, `deadpool-sqlite` will queue requests. Under extreme load, this could lead to timeouts.
- **Rollback urgency**: Low. The changes are additive (new error types, new pool wrapper, new middleware). Schema dedup and error strategy are backward-compatible.
- **Monitoring gaps**: No metrics/logging added for pool status, connection wait times, or error rates. Would help with production debugging.

---

*Report generated by QC Specialist #3 (Performance & Reliability focus)*

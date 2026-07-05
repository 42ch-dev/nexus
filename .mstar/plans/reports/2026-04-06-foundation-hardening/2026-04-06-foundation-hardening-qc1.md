---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-04-06-foundation-hardening"
verdict: "Approve"
generated_at: "2026-04-06"
---

# QC Report #1 — Foundation Hardening (Plan A)

**Reviewer**: QC Specialist #1
**Date**: 2026-04-06
**Branch**: feature/v2.0-foundation-hardening
**Commit**: 1b8e849

## Summary

Plan A delivers solid foundation hardening with proper error strategy, connection pooling, schema centralization, workspace middleware, and unwrap cleanup. The implementation follows Rust best practices and the existing codebase patterns. No critical runtime issues found. Clippy passes clean. Test coverage is comprehensive with integration tests validating actual behavior.

**Overall Quality**: High. Production-ready with minor improvements recommended.

## Findings

### Critical (Must Fix)

None identified. The implementation is sound and follows established patterns.

### High (Should Fix)

None identified. The code handles errors correctly and the pool lifecycle is properly managed.

### Medium (Consider Fixing)

- **[M-1] Mutex poisoning uses `.expect()` in production path**
  - **Location**: `crates/nexus42d/src/workspace/mod.rs:78, 86, 131`
  - **Issue**: `workspace_path.lock().expect("workspace_path mutex poisoned")` will panic if a thread holding the mutex panicked. This is intentional crash-on-poison policy, but should be documented as design decision.
  - **Risk**: If the `init_workspace()` async task panics while holding the mutex, subsequent requests to `is_initialized()` or `workspace_path()` will crash the daemon.
  - **Recommendation**: Add comment explaining crash-on-poison rationale, or consider graceful recovery with `lock().unwrap_or_else(|e| e.into_inner())`.
  - **Severity**: Medium (rare but potential daemon crash)

- **[M-2] Pool connection acquisition has no timeout**
  - **Location**: `crates/nexus42d/src/db/pool.rs:47-49`
  - **Issue**: `pool.get().await` uses deadpool's default timeout (30s based on pool config). If pool is exhausted, handlers wait indefinitely until timeout.
  - **Risk**: Under extreme load, requests queue indefinitely. The current pool size (8) for SQLite WAL is reasonable for a local daemon, but edge cases exist.
  - **Recommendation**: Consider adding explicit timeout configuration or documenting the 30s default in code comments.
  - **Severity**: Medium (edge case, not likely for local daemon)

- **[M-3] Error code inconsistency between Internal variant and From impls**
  - **Location**: `crates/nexus42d/src/api/errors.rs:60-67, 102, 112`
  - **Issue**: `error_code()` returns `"INTERNAL"` for `Internal { .. }`, but `From<anyhow::Error>` produces `"INTERNAL_ERROR"` and `From<rusqlite::Error>` produces `"DATABASE_ERROR"` as the `code` field in the variant.
  - **Impact**: The `ApiErrorResponse.error.code` field will contain `"INTERNAL"` (from `error_code()`) while the internal variant's `code` field has `"INTERNAL_ERROR"` or `"DATABASE_ERROR"`. This is the intended behavior (surface-level code vs. detailed diagnostic code), but it's confusing without documentation.
  - **Recommendation**: Add doc comment on `Internal { code, message }` explaining that `code` is diagnostic detail while `error_code()` returns surface-level `"INTERNAL"` for API consumers.
  - **Severity**: Medium (documentation gap, not a bug)

### Low (Suggestions)

- **[L-1] Pool builder `.expect()` comment could be more explicit**
  - **Location**: `crates/nexus42d/src/db/pool.rs:36`
  - **Issue**: `.expect("builder() is infallible for valid Runtime")` — the comment states infallibility but doesn't explain WHY it's infallible (deadpool's `Config::builder()` only fails on invalid `Runtime` enum, and `Runtime::Tokio1` is always valid).
  - **Recommendation**: Expand comment: `"builder() only fails on invalid Runtime enum; Tokio1 is valid by definition"`.
  - **Severity**: Low (documentation improvement)

- **[L-2] CLI schema lacks explanation for outbox exclusion**
  - **Location**: `crates/nexus42/src/db/mod.rs:25-26`
  - **Issue**: The CLI schema comment says "Does NOT create daemon-only tables (outbox)" but doesn't explain WHY. The daemon schema has extensive documentation on the difference between `outbox_entries` (sync) vs daemon `outbox` (command queue).
  - **Recommendation**: Add brief note: "CLI does not maintain sync outbox; that is handled by nexus-sync crate using separate `outbox_entries` schema."
  - **Severity**: Low (documentation gap)

- **[L-3] InvalidInput error struct could expose field-level details**
  - **Location**: `crates/nexus42d/src/api/errors.rs:31-32, 71-79`
  - **Issue**: `InvalidInput { field, reason }` stores structured validation details, but `to_response_body()` flattens them into a single `message: "Invalid input: {reason}"`. Client-side validation could benefit from structured `field` information.
  - **Recommendation**: Consider adding `field` to `ApiErrorDetail` for `INVALID_INPUT` errors, or document that structured validation will be added in future iteration.
  - **Severity**: Low (nice-to-have, not blocking)

- **[L-4] Pool size hardcoded, not configurable**
  - **Location**: `crates/nexus42d/src/db/pool.rs:12, 55`
  - **Issue**: `DEFAULT_POOL_SIZE: usize = 8` is hardcoded. For SQLite WAL, 8 is reasonable, but for different deployment scenarios (low-resource environments), configurability would be valuable.
  - **Recommendation**: Consider adding pool size to daemon config (future iteration).
  - **Severity**: Low (acceptable for V1.0 local daemon)

### Cross-Reviewer Ready Notes

**Integration & Migration Risks**:
1. The error strategy introduces a new contract (`ApiErrorResponse`) that CLI must parse. Current `daemon_client.rs` correctly handles structured errors via `DaemonErrorResponse` parsing. ✅ Verified compatible.

2. The middleware changes route behavior: previously `/v1/local/creators` would return empty list on uninitialized workspace; now returns 409 Conflict. This is a **breaking change** for CLI behavior. The plan correctly documents this as intentional (workspace initialization guard). Integration tests verify the new behavior.

3. Schema deduplication: CLI and daemon schemas are now identical for shared tables (`workspace_meta`, `creators`, `reference_sources`). Migration: both crates use `SCHEMA_VERSION = "1"` — no schema migration needed. ✅ Verified identical.

**Testing Evidence**:
- Middleware tests (`api/middleware.rs`) verify unguarded routes return 200 without init, guarded routes return 409 without init, and guarded routes succeed after init. ✅ Comprehensive.
- Pool tests (`db/pool.rs`) verify concurrent access, connection lifecycle, and pool creation. ✅ Solid.
- Error tests (`api/errors.rs`) verify status code mapping, response body structure, and From impl chain preservation. ✅ Complete.
- Integration tests (`tests/integration.rs`) verify concurrent handler requests succeed. ✅ Realistic load simulation.

**Long-term Evolution Risks**:
1. The mutex crash-on-poison policy in `WorkspaceState` could cause daemon instability if async tasks panic. Mitigation: consider panic hook logging or graceful mutex recovery in future iterations.

2. The pool has no explicit health check or connection validation. If SQLite connection becomes corrupted (e.g., disk error), pool returns broken connections. Mitigation: consider adding connection validation on checkout in future.

3. The error strategy uses `Internal { code, message }` with arbitrary codes. Future error cataloging (error registry for monitoring) would benefit from structured error taxonomy. Recommendation: document that error codes will be enumerated in future iteration.

### Correctness Verification

- **NexusApiError status codes**: ✅ Verified via tests in `errors.rs:122-161`
  - `Uninitialized → 409 Conflict` (correct for precondition failure)
  - `InvalidInput → 400 Bad Request` (correct)
  - `Internal → 500 Internal Server Error` (correct)
  - `AuthRequired → 401 Unauthorized` (correct)
  - `NotFound → 404 Not Found` (correct)

- **Middleware application**: ✅ Verified via tests in `middleware.rs`
  - Unguarded: `runtime/health`, `runtime/status`, `workspace`, `workspace/init`, `auth/status` — return 200 without init
  - Guarded: `creators`, `manuscript`, `references`, `context/assemble` — return 409 without init, 200 after init

- **Pool lifecycle**: ✅ Verified via tests in `pool.rs`
  - Connection returned to pool after drop (verified by pool.status().size)
  - Concurrent access works (4 concurrent tasks succeed)
  - Connection creation succeeds

- **Schema identity CLI/daemon**: ✅ Verified by reading both files
  - `workspace_meta`, `creators`, `reference_sources` — identical definitions
  - `SCHEMA_VERSION = "1"` — both match
  - CLI excludes `outbox` (daemon-only) — correctly excluded

### Security Verification

- **No sensitive data in error messages**: ✅ Verified
  - `Internal { message }` uses `.to_string()` on errors — may include stack traces in debug mode, but production release builds strip stack traces
  - Database errors show rusqlite error messages — safe for local daemon (not external API)
  - No hardcoded credentials or secrets visible

- **No SQL injection vectors**: ✅ Verified
  - All SQL uses parameterized queries via `rusqlite::params![]` or `P: rusqlite::Params`
  - `query_map`, `query_row`, `execute` all use prepared statements with params
  - Schema definitions use `IF NOT EXISTS` — safe

- **Mutex poisoning**: ⚠️ Reviewed
  - Uses `.expect()` which panics on poison — intentional crash-on-poison policy
  - Not a security issue, but reliability concern (see M-1)

### Performance Verification

- **Pool size (8)**: ✅ Reasonable for SQLite WAL
  - SQLite WAL mode supports concurrent readers
  - Local daemon typically has low concurrent requests
  - 8 connections allows ~8 concurrent handlers without queueing

- **SyncWrapper overhead**: ✅ Acceptable
  - `PooledConn::interact()` executes on blocking thread pool
  - Each async method spawns blocking task — acceptable for SQLite (not high-frequency hot path)
  - Alternative: dedicated thread-per-connection would add complexity without benefit for this use case

- **No unnecessary allocations**: ✅ Verified
  - Error messages use `.to_string()` on error chain — unavoidable for error formatting
  - SQL strings cloned for async closure capture (standard pattern for `interact()`)
  - No obvious allocation waste

### Code Quality Verification

- **Follows existing patterns**: ✅
  - Error strategy mirrors standard axum error handling (IntoResponse, thiserror)
  - Pool mirrors standard deadpool-sqlite usage
  - Schema follows existing schema.rs pattern from prior work

- **Names clear and accurate**: ✅
  - `NexusApiError` — clear domain name
  - `DbPool`, `PooledConn` — clear type names
  - `require_workspace` middleware — clear intent
  - `SCHEMA_VERSION`, `DEFAULT_POOL_SIZE` — clear constants

- **No dead code**: ✅
  - `#[allow(dead_code)]` on CLI types with justification (`context/types.rs:51`, `summary.rs:12`)
  - Integration test helper `TestApp` correctly implements Deref

- **Test coverage adequate**: ✅
  - Unit tests for each component
  - Integration tests for middleware, pool, error handling
  - Concurrent access tested
  - Edge cases tested (empty input, uninitialized workspace, pool exhaustion not tested but acceptable)

### Test Quality Verification

- **Tests verify actual behavior**: ✅
  - Middleware tests use `axum_test::TestServer` with real router
  - Pool tests use real SQLite database (temp files)
  - Error tests use real IntoResponse conversion

- **Edge cases covered**: ✅
  - Empty path validation (`init_workspace_with_empty_path_returns_400`)
  - Uninitialized workspace (all guarded routes)
  - Concurrent pool access
  - Pool creation failure (implicit via DbPool::new error handling)

- **Integration tests realistic**: ✅
  - `concurrent_handler_requests_succeed` simulates realistic concurrent load
  - Middleware tests use production router setup
  - Database seeded with realistic test data

## Verdict

**APPROVE** — The implementation is high quality, follows best practices, and passes all lint checks. No critical issues found. Medium-severity findings are documentation/design-decision gaps, not bugs. Recommend addressing M-1 (mutex poisoning policy) before merge to ensure reliability under edge-case failure scenarios. M-2 and M-3 are acceptable for V1.0 but worth documenting for future iterations.

**Confidence**: High. Evidence from clippy, comprehensive tests, and code review.

**Blocking Issues**: None.

**Recommended Before Merge**: Address M-1 (add comment explaining mutex poison crash policy or consider graceful recovery).

**Cross-Reviewer Validation Points** (for QC #2 and #3 to verify):
1. Verify `NexusApiError::status_code()` matches HTTP semantics (409 for precondition failure is correct per RFC 7231 §6.5.8)
2. Verify middleware does NOT leak to unguarded routes (integration tests confirm)
3. Verify pool handles `InteractError::Panic` correctly (current impl converts to rusqlite::Error — acceptable but may lose panic context)
4. Verify `daemon_client.rs` correctly parses new structured errors (yes, `DaemonErrorResponse` matches `ApiErrorResponse`)
5. Verify schema drift risk: both CLI and daemon use same `SCHEMA_VERSION` constant — confirm no drift in future updates
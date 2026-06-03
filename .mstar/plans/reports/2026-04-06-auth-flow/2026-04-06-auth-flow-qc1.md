---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-04-06-auth-flow"
verdict: "Approve"
generated_at: "2026-04-07"
---

# QC Review #1: Auth Flow Completion (Plan D)

**Reviewer**: @qc-specialist (Reviewer #1)
**Primary Accent**: Architecture consistency, maintainability, long-term evolution risks
**Secondary Accent**: Correctness and basic security risks

## Executive Summary

The implementation delivers the core auth flow functionality: device code OAuth, token lifecycle management, and daemon auth middleware. The code is well-structured with comprehensive test coverage. All SQL queries use parameterized statements, clippy passes clean, and there are no `.unwrap()` calls in production code paths.

**Verdict: APPROVE** with 2 medium-severity documentation items to address before merge.

## Review Checklist

| Item | Status | Notes |
|------|--------|-------|
| No `.unwrap()` in production code | ✅ PASS | All unwraps are in `#[cfg(test)]` blocks |
| auth_tokens table schema | ✅ PASS | Correct columns: user_id, access_token, refresh_token, expires_at, created_at |
| Tokens not stored in plaintext (or documented) | ⚠️ MEDIUM | Plaintext storage; needs risk documentation |
| Middleware returns 401 for invalid/missing tokens | ✅ PASS | `NexusApiError::AuthRequired` → 401 |
| Device code polling has timeout and max attempts | ✅ PASS | `max_attempts = min(expires_in/interval, 60)` |
| SQL queries use parameterized statements | ✅ PASS | All queries use `?1, ?2, ...` bindings |
| Tests cover error paths | ✅ PASS | Expired, invalid, missing token tests present |
| Auth routes excluded from require_auth | ✅ PASS | `/v1/local/auth/*` routes are unguarded |

## Findings

| ID | Severity | Location | Description | Decision |
|----|----------|----------|-------------|----------|
| AUTH-M1 | MEDIUM | `token_manager.rs` header | Token plaintext storage undocumented — needs security model comment | Defer V1.1 — document risk |
| AUTH-M2 | MEDIUM | `token_manager.rs:26-33` | Token refresh not implemented — only `needs_refresh()` detection | Defer V1.1 — infrastructure ready |
| AUTH-L1 | LOW | `handlers/auth.rs:171-176` | Device code session cleanup ignores errors | Accept |
| AUTH-L2 | LOW | `handlers/auth.rs:214-219` | Mock user code uses pseudo-random (mock-only) | Accept |

## POSITIVE Observations

1. SQL Injection Prevention: All queries use parameterized statements
2. Error Propagation: Proper use of `NexusApiError` with structured error codes
3. Test Coverage: 25+ tests covering edge cases (expired, invalid, missing tokens)
4. Route Exclusion: Auth routes correctly excluded from middleware
5. Device Code Polling: Proper timeout with max_attempts cap
6. Architecture Alignment: CLI delegates to daemon API (no local SQLite)

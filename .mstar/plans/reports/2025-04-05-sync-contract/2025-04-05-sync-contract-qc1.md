# QC Review #1 — sync-contract

**Reviewer**: @qc-specialist
**Date**: 2026-04-06
**Scope**: nexus-sync crate (10 files) + 2 schema files + workspace changes

## Summary

REQUEST CHANGES — 1 Critical (formatting), 1 High (unwrap), 2 High (suggestions), 5 Medium, 3 Low.

The `nexus-sync` crate implementation is well-structured, follows the plan requirements, and anchors to generated contracts from `nexus-contracts`. The code demonstrates solid Rust practices with proper error handling, parameterized SQL queries, and comprehensive tests. There is **one blocking issue** (formatting violation) and several medium-priority suggestions for robustness improvements.

**Evidence gathered**:
- Clippy: **Clean** (0 warnings with `-D warnings`)
- Format: **FAILED** — `sync_client.rs` has 7 formatting violations (line length)
- Tests: 226 tests passing (133 domain + 70 sync + 16 CLI + 7 daemon)
- SQL injection: **Safe** — all queries use parameterized bindings (`?1`, `?2`)
- Security: **Safe** — no hardcoded secrets/tokens found
- Unwrap/expect in production: **1 instance** (sync_client.rs:230) — needs fix

---

## Critical (Blocking)

### C-1: Formatting Violation in sync_client.rs

**File**: `crates/nexus-sync/src/sync_client.rs`
**Evidence**: `cargo fmt --check -p nexus-sync` output shows 7 diffs

The file contains long lines that violate `rustfmt` rules. Lines 137, 176, 134, 173, 252-275, 314.

**Impact**: CI `cargo fmt --check` will fail. AGENTS.md mandates `cargo +nightly fmt --all` before commit.

**Fix**: Run `cargo +nightly fmt -p nexus-sync`.

---

## High (Should Fix)

### H-1: Unsafe unwrap() in Production Code Path

**File**: `crates/nexus-sync/src/sync_client.rs:230`
**Code**: `Err(SyncError::HttpError(last_error.unwrap()))`

After retry loop exhausts all attempts, `last_error` is unwrapped without guarantee. Use `last_error.expect("retry loop exhausted without transient error")` or restructure.

### H-2: Missing Test for Outbox Transaction Atomicity

**File**: `crates/nexus-sync/src/outbox.rs`

No tests verifying: transaction rollback on partial failure, concurrent write safety, crash recovery.

**Fix**: Add integration tests for concurrent `append()` calls and transaction rollback.

---

## Medium (Suggestions)

### M-1: BundleBuilder::build() Missing Delta Sequence Monotonicity Validation

**File**: `crates/nexus-sync/src/delta_bundle.rs:226-231`

`build()` validates `deltas.is_empty()` and `submitting_creator_id`, but does NOT validate delta sequence monotonicity or `base_versions["world_revision"]` presence.

### M-2: Precheck Severity Should Differentiate Blocking vs Advisory

**File**: `crates/nexus-sync/src/precheck.rs:90-106`

`PrecheckSeverity::Error` used for both schema violations (blocking) and ID prefix mismatches (could be advisory).

### M-3: SyncClient::new() Should Validate Auth Token Format

**File**: `crates/nexus-sync/src/sync_client.rs:81-85`

`new()` checks for empty token but doesn't validate format.

### M-4: Outbox Schema Migration Path Not Documented

**File**: `crates/nexus-sync/src/outbox.rs:81-104`

No versioning/migration mechanism for future schema changes.

### M-5: ConflictResponse::from_json() Defaults to Hard Failure

**File**: `crates/nexus-sync/src/conflict.rs:152-155`

Missing `conflict_type` defaults to `hard_validation_failure` — should return error if required field missing.

---

## Low (Nitpicks)

### L-1: Unused Variable Warning Suppressed
**File**: `crates/nexus-sync/src/command.rs:98` — `let _workspace_id = cmd.workspace_id.clone();`

### L-2: Inconsistent Error Message Style
**Files**: `errors.rs`, `outbox.rs` — Mix of `{id}` and `{0}` syntax.

### L-3: PrecheckReport Display Format Could Be More Structured
**File**: `crates/nexus-sync/src/precheck.rs:84-88`

---

## Positive Observations

1. Schema Anchoring: Correctly uses generated types — no duplicate DTOs
2. Bundle Metadata Fields: Correctly implements SYNC-R1
3. SQL Safety: All 12 queries use parameterized bindings
4. Error Design: Comprehensive `SyncError` with `thiserror`
5. Test Coverage: 70 tests with edge cases
6. Precheck: All 6 validation stages implemented
7. Retry Logic: Exponential backoff — production-ready

---

## Compliance Check

| Frozen Constraint | Status |
|---|---|
| JSON Schema as truth source | ✅ PASS |
| No Neo4j/Postgres/pgvector on CLI | ✅ PASS |
| CLI is ACP client not agent/server | ✅ PASS |
| Bundle metadata fields | ✅ PASS |

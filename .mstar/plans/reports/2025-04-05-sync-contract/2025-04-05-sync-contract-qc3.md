# QC Review #3 — sync-contract

**Reviewer**: @qc-specialist-3
**Date**: 2026-04-06
**Scope**: nexus-sync crate — security, API contract, spec compliance

## Summary

APPROVE WITH CONDITIONS — 3 Critical, 4 High, 5 Medium, 3 Low.

The implementation is solid overall with good test coverage and follows the plan contract well. However, there are several issues that should be addressed, ranging from critical bugs to medium-priority improvements.

---

## Critical (Blocking)

### C-1: Incomplete Delta Validation in cli-sync Schema

**File**: `schemas/cli-sync/bundle.schema.json` — Lines 27-36

The `deltas` array definition only validates `delta_type` but the domain schema requires `delta_type`, `operation`, `payload`, AND `local_timestamp` as required fields.

**Fix**: Update cli-sync schema to include full delta item validation.

### C-2: Potential Panic in ConflictDetail Parsing

**File**: `crates/nexus-sync/src/conflict.rs` — Lines 232-234

`parse_conflict_detail` uses `?.as_str()?.to_string()` chain. If server returns `"code": 123` instead of string, this will panic.

**Fix**: Add type checking before conversion.

### C-3: Fragile Conflict Detection in HTTP Response

**File**: `crates/nexus-sync/src/sync_client.rs` — Line 139

Using string search `body.contains("\"success\": false")` is fragile — false positives/negatives on whitespace variations.

**Fix**: Always parse JSON first and check `success` field properly.

---

## High (Should Fix)

### H-1: No Validation that submitting_creator_id Matches Authenticated Creator

**File**: `crates/nexus-sync/src/precheck.rs`

Precheck only validates format (prefix), not actual match against authenticated identity. Plan specifies match required for V1.0.

### H-2: SyncClient Has No Request Body Size Limit

**File**: `crates/nexus-sync/src/sync_client.rs:87-89`

`Client::builder()` does not set max request body size. Could cause OOM.

### H-3: mark_sent Allows Invalid State Transitions

**File**: `crates/nexus-sync/src/outbox.rs:205-222`

`mark_sent` allows transitioning from `'failed'` state, re-sending after conflict without user intervention.

### H-4: Exponential Backoff Can Panic at High Retry Counts

**File**: `crates/nexus-sync/src/outbox.rs:299`

`2u64.pow(retry_count as u32)` panics if retry_count > 63. Use `saturating_pow()`.

---

## Medium (Suggestions)

### M-1: No Retry Logic Based on Conflict Response retry_after Field
### M-2: Precheck Doesn't Validate submitting_creator_id Equals creator_id
### M-3: AutoReject Resolution Has No Safety Mechanism
### M-4: Partial Apply Doesn't Persist Retry State
### M-5: Replay Doesn't Return bundle_payload

---

## Low (Nitpicks)

### L-1: Workspace ID Prefix Validation Uses Warning Instead of Error
### L-2: Error Message Information Leakage
### L-3: Unused command_payload Field in Outbox

---

## Positive Observations

1. Excellent test coverage with good edge case coverage
2. Proper use of thiserror — Error types follow domain patterns
3. Good SQLite practices — WAL mode, proper indexes, parameterized queries
4. Correct bundle metadata fields per plan
5. Proper ConflictType enum coverage per spec
6. Partial apply semantics sound
7. Schema files are valid JSON Schema
8. Cargo clippy passes — No warnings

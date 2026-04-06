# QC Review #2 — sync-contract

**Reviewer**: @qc-specialist-2
**Date**: 2026-04-06
**Scope**: nexus-sync crate — architecture, test quality, consistency, concurrency safety, schema fidelity

## Summary

REQUEST CHANGES — 2 Critical, 2 High, 3 Medium, 2 Low.

The `nexus-sync` crate implementation is well-structured with good module separation and comprehensive test coverage. The error patterns align with the domain layer, and the schema contracts are correctly defined. However, there are critical concurrency safety issues in the SQLite outbox implementation and fragile conflict detection logic that must be addressed before merge.

**Clippy Status**: ✅ Clean
**Test Coverage**: ✅ Good (80%+ estimated)

---

## Critical (Blocking)

### C-1: Outbox SQLite operations lack transaction wrapping

**File**: `crates/nexus-sync/src/outbox.rs` — Lines 140-169 (`append`), 171-193 (`stage`)

`append()` and `stage()` perform INSERT operations without explicit transaction wrapping. Concurrent calls from multiple tokio tasks could lead to race conditions.

**Fix**: Wrap write operations in explicit transactions with `self.conn.transaction()`.

### C-2: Conflict detection uses fragile string matching

**File**: `crates/nexus-sync/src/sync_client.rs` — Lines 129-138

Conflict detection parses response body as text and checks for substring `"success": false`. This is brittle — whitespace variations or nested occurrences would break detection.

**Fix**: Deserialize as `ConflictResponse` first, check `success` field properly.

---

## High (Should Fix)

### H-1: ConflictResponse::from_json silently defaults success to false

**File**: `crates/nexus-sync/src/conflict.rs` — Lines 134-139

Missing `success` field defaults to `false` instead of returning an error. Malformed responses treated as conflicts.

**Fix**: Return error if `success` field is missing or not a boolean.

### H-2: Duplicate create detection has hash collision bug

**File**: `crates/nexus-sync/src/precheck.rs` — Lines 382-397

Creates without `target_entity_id` all hash to the same key `"world:None"`, allowing duplicate creates to slip through.

**Fix**: Only check duplicates for creates WITH target_id.

---

## Medium (Suggestions)

### M-1: Outbox retry backoff formula may overflow for large retry counts
**File**: `crates/nexus-sync/src/outbox.rs:55-56, 297-299` — Use saturating arithmetic.

### M-2: PushResponse contains unused fields
**File**: `crates/nexus-sync/src/sync_client.rs:28-44` — `data_freshness_hint` and `last_indexed_bundle_id` defined but never consumed.

### M-3: Schema allOf reference may not enforce constraints correctly
**File**: `schemas/cli-sync/bundle.schema.json:8-11` — `allOf` + `properties` doesn't override domain schema constraints.

---

## Low (Nitpicks)

### L-1: Magic number for max HTTP retries
**File**: `crates/nexus-sync/src/sync_client.rs:23` — Add comment explaining choice.

### L-2: Test uses hard-coded timestamp format
**File**: `crates/nexus-sync/src/command.rs:255`

---

## Positive Observations

1. Excellent module separation with clear boundaries
2. Comprehensive test coverage across all modules
3. Schema-driven design with correct domain contract references
4. Error type consistency with `thiserror` pattern
5. BundleBuilder fluent API is ergonomic
6. Partial apply semantics correct per roadmap
7. Precheck validation catches real issues

---

## Evidence Traceability

- C-1: `outbox.rs:140-169`, `outbox.rs:171-193`
- C-2: `sync_client.rs:129-138`
- H-1: `conflict.rs:134-139`
- H-2: `precheck.rs:382-397`
- M-1: `outbox.rs:55-56, 297-299`
- M-2: `sync_client.rs:37-38`
- M-3: `schemas/cli-sync/bundle.schema.json:8-11`

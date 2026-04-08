# Sync Contract Implementation Plan (V1.0-phase1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the sync mechanism for CLI ↔ Platform synchronization using Command, DeltaBundle, and Outbox pattern.

**Architecture:** 
- **Command**: User-initiated operations (create KB, update memory, etc.)
- **DeltaBundle**: Batch of deltas sent to platform
- **Outbox**: Local queue of pending operations for offline-first sync
- **Conflict Resolution**: Optimistic locking with `world_revision` and `last_confirmed_delta_sequence`

**Tech Stack:** Rust 1.75+, serde, tokio, reqwest (HTTP client), uuid

**Branch Strategy:** Feature branch `feature/v1.0-sync-contract` from `main`

**Architecture Review:** See `.agents/plans/reports/2025-04-05-phase1-review/2025-04-05-phase1-architect-review.md` for detailed review and constraints.

---

## Working Branch

**Branch:** `feature/v1.0-sync-contract`
**Base:** `main` (after domain-models complete)

---

## Core Tasks Overview

### Task 1: Create Sync Crate (Schema-Anchored)

**IMPORTANT**: Use generated types from `crates/nexus-contracts` (generated from `schemas/`). Do NOT hand-write duplicate types.

- Create `crates/nexus-sync/Cargo.toml`
- Add dependency on `nexus-contracts` crate
- **Anchor to JSON Schema truth source**:
  - Bundle envelope: `schemas/cli-sync/bundle.schema.json`
  - Delta types: `schemas/cli-sync/bundle.schema.json` (delta definitions)
  - Common types: `schemas/common/`
- Define Command types (CreateKB, UpdateKB, DeleteKB, CreateMemory, etc.)
- Define DeltaBundle builder using generated types

### Task 2: Implement Outbox Pattern

- Create `Outbox` struct for local operation queue
- Implement append, serialize, and replay operations
- **Persistence recommendation**: Use SQLite (`$HOME/.nexus42/state.db`) instead of JSON file for:
  - Atomic transactions
  - Crash consistency
  - Alignment with CLI SQLite usage (per `restructured-context-assembly-v1.md` §2.3)
- If using JSON file for V1.0: `$HOME/.nexus42/outbox.json` (add migration note for future SQLite migration)

### Task 3: Implement Bundle Builder (NEW: Bundle Metadata Fields)

**Critical**: Add V1.0 frozen bundle metadata fields (SYNC-R1).

- Aggregate commands into DeltaBundle
- Calculate `last_confirmed_delta_sequence`
- **Add bundle-level fields**:
  - `submitting_creator_id: CreatorId` — Identifies which Creator submitted this bundle
  - `manuscript_phase: ManuscriptPhase` — Current manuscript lifecycle phase (brainstorm/draft/review/finalize/published)
  - `output_manuscript: bool` — Whether this execution requires manuscript output
- **Add `story_manifest` delta type support**:
  - Required for context-assembly summary payload
  - Payload includes `summary_text` field
- Add bundle signing (future: cryptographic signature)

### Task 4: Implement Sync Client

- HTTP client to platform sync API
- Bundle upload with retry logic
- Conflict detection and resolution
- **Parse `SyncConflictResponseV1`** from `schemas/cli-sync/conflict-response.schema.json` (per `hard-vs-soft-validation-v1.md` §7)

### Task 5: Implement Conflict Resolution

- Detect `version_mismatch` and sequence conflicts
- Implement manual vs automatic resolution strategies
- Add conflict logging for user review

### Task 6: Partial Apply Semantics (NEW — Resolves SYNC-R2)

**Goal**: Handle Phase A/B partial success per roadmap §3.1.4 (P1).

- Parse `bundle_apply_status: "partial"` response
- Distinguish between:
  - Phase A succeeded, Phase B (projection/indexing) failed
  - Total failure
- Store partial apply state for retry
- Expose `data_freshness_hint` / `last_indexed_bundle_id` to caller (CLI)

### Task 7: Local Precheck Stage (NEW — Resolves SYNC-R3)

**Goal**: Validate bundle locally before HTTP upload (quality improvement).

- **Stage 0 (Precheck)**: Before bundle upload
  - Validate command consistency (no conflicting operations)
  - Validate schema compliance (all required fields present)
  - Validate sequencing (`last_confirmed_delta_sequence` monotonic)
  - Check `world_revision` against local state
- Reject invalid bundles early (before platform round-trip)
- Log precheck failures with actionable error messages

---

## Files to Create

**Sync Crate (`crates/nexus-sync/`):**
- `Cargo.toml`
- `src/lib.rs`
- `src/command.rs` (Command types)
- `src/delta_bundle.rs` (Bundle builder with metadata fields)
- `src/outbox.rs` (Local operation queue)
- `src/sync_client.rs` (Platform sync client)
- `src/conflict.rs` (Conflict resolution)
- `src/partial_apply.rs` (NEW: Partial apply handling)
- `src/precheck.rs` (NEW: Local precheck stage)
- `src/errors.rs`

**Schema Updates** (if fields missing):
- `schemas/cli-sync/bundle.schema.json` — Add `submitting_creator_id`, `manuscript_phase`, `output_manuscript` if not present
- `schemas/cli-sync/bundle.schema.json` — Ensure `story_manifest` delta type is defined

---

## Verification

- [x] Sync crate compiles: `cargo build -p nexus-sync`
- [x] Command types serialize/deserialize correctly
- [x] Outbox persists and replays operations
- [x] Bundle builder creates valid DeltaBundle with metadata fields
- [x] Sync client makes HTTP requests (mock server for testing)
- [x] Partial apply response parsing works: mock `bundle_apply_status: "partial"`
- [x] Local precheck rejects invalid bundles: unit tests
- [x] Integration test with CLI daemon: bundle upload flow

---

## Bundle Metadata Field Contract

| Field | Type | Purpose | Spec Anchor |
|-------|------|---------|-------------|
| `submitting_creator_id` | `CreatorId` | Identifies which Creator submitted this bundle | roadmap §3.1.1, §3.1.2 (Creator first-class citizen) |
| `manuscript_phase` | `ManuscriptPhase` | Current manuscript lifecycle phase | roadmap §3.1.1, `data-model-v1.md` |
| `output_manuscript` | `bool` | Whether this execution requires manuscript output | `story-manifest.schema.json`, `restructured-context-assembly-v1.md` §3.4 |

**Dependency**: These fields are prerequisites for `context-assembly` plan (unblocks CTX-R1).

---

## Architecture Constraints (From Review)

| Constraint | Source | Compliance |
|------------|--------|------------|
| Rust-first for sync library | AGENTS.md | ✅ |
| JSON Schema as wire truth source | `codegen-strategy-v1.md` | ✅ (Task 1: anchor to schemas, use generated types) |
| CLI uses SQLite for local state | `restructured-context-assembly-v1.md` §2.3 | ✅ (Task 2: SQLite outbox) |
| No Neo4j/Postgres/pgvector on CLI side | `restructured-context-assembly-v1.md` §2.3 | ✅ |
| V1.0 `submitting_creator_id` | roadmap §3.1.1, §3.1.2 | ✅ (Task 3) |
| V1.0 `manuscript_phase` | roadmap §3.1.1 | ✅ (Task 3) |
| Phase A/B partial apply | roadmap §3.1.4 (P1) | ✅ (Task 6) |
| HTTP conflict response: `200 + success: false + conflict body` | `hard-vs-soft-validation-v1.md` §7 | ✅ (Task 4) |

---

## Dependency on Other Plans

- **cli-daemon-foundation**: Requires dual-subject auth (CLI-R4 resolution) for `submitting_creator_id` to be meaningful
- **context-assembly**: Blocked until this plan delivers `story_manifest` delta + bundle metadata fields (SYNC-R1)

**Parallel execution**: Tasks 1–3 (library-only) can run in parallel with cli-daemon Tasks 1–4. Tasks 4–7 (client + precheck + partial apply) require daemon Local API endpoint.

---

**Plan saved to:** `.agents/plans/2025-04-05-sync-contract.md`  
**Last updated:** 2026-04-06 (Architecture Review: Request Changes → Expanded per §6.2)
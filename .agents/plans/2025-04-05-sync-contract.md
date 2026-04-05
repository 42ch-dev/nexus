# Sync Contract Implementation Plan (Phase 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the sync mechanism for CLI ↔ Platform synchronization using Command, DeltaBundle, and Outbox pattern.

**Architecture:** 
- **Command**: User-initiated operations (create KB, update memory, etc.)
- **DeltaBundle**: Batch of deltas sent to platform
- **Outbox**: Local queue of pending operations for offline-first sync
- **Conflict Resolution**: Optimistic locking with `world_revision` and `last_confirmed_delta_sequence`

**Tech Stack:** Rust 1.75+, serde, tokio, reqwest (HTTP client), uuid

**Branch Strategy:** Feature branch `feature/v1.0-sync-contract` from `main`

---

## Working Branch

**Branch:** `feature/v1.0-sync-contract`
**Base:** `main` (after domain-models complete)

---

## Core Tasks Overview

### Task 1: Create Sync Crate
- Create `crates/nexus-sync/Cargo.toml`
- Define Command types (CreateKB, UpdateKB, DeleteKB, CreateMemory, etc.)
- Define DeltaBundle builder

### Task 2: Implement Outbox Pattern
- Create `Outbox` struct for local operation queue
- Implement append, serialize, and replay operations
- Add persistence to `$HOME/.nexus42/outbox.json`

### Task 3: Implement Bundle Builder
- Aggregate commands into DeltaBundle
- Calculate `last_confirmed_delta_sequence`
- Add bundle signing (future: cryptographic signature)

### Task 4: Implement Sync Client
- HTTP client to platform sync API
- Bundle upload with retry logic
- Conflict detection and resolution

### Task 5: Implement Conflict Resolution
- Detect `version_mismatch` and sequence conflicts
- Implement manual vs automatic resolution strategies
- Add conflict logging for user review

---

## Files to Create

**Sync Crate (`crates/nexus-sync/`):**
- `Cargo.toml`
- `src/lib.rs`
- `src/command.rs` (Command types)
- `src/delta_bundle.rs` (Bundle builder)
- `src/outbox.rs` (Local operation queue)
- `src/sync_client.rs` (Platform sync client)
- `src/conflict.rs` (Conflict resolution)
- `src/errors.rs`

---

## Verification

- [ ] Sync crate compiles: `cargo build -p nexus-sync`
- [ ] Command types serialize/deserialize correctly
- [ ] Outbox persists and replays operations
- [ ] Bundle builder creates valid DeltaBundle
- [ ] Sync client makes HTTP requests (mock server for testing)

---

**Plan saved to:** `.agents/plans/2025-04-05-sync-contract.md`
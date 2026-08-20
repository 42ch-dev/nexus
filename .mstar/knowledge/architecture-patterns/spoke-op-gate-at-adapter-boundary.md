---
module: nexus-spoke-adapter
date: 2026-08-14
problem_type: knowledge
category: architecture-patterns
severity: medium
tags: [spoke, adapter-boundary, layering, validate-gate, pure-storage, sole-consumer, mind-state]
last_updated: 2026-08-14
applies_when: Adding any storage that is written through a spoke-operation validator (validate_* helpers, future mind-axis ops); deciding where a validate gate belongs; any PR that would add spoke-schemas/spoke-operations to a crate other than nexus-spoke-adapter
consolidation_review: spoke-adapter-port-orchestration-adoption.md (Surface B) + spoke-adapter-conversion-seam.md (Surface A) — same adapter-boundary family; review together at next compound-refresh
---

# Spoke-Op Validation Gates Live at the Adapter Boundary

> Complements [`spoke-adapter-port-orchestration-adoption.md`](spoke-adapter-port-orchestration-adoption.md) (Surface B) and [`spoke-adapter-conversion-seam.md`](spoke-adapter-conversion-seam.md) (Surface A). This doc covers the **layering rule** those two assume: which crate may depend on spoke-operations.

## Context

V1.164 P2 added the `mind_states` table. The first implementation (a10d5e4e) put `spoke_operations::validate_mind_state` **inside `nexus-local-db`** (new direct dep) — "the validator gates the write path, so it lives with the writer." That violated the normative layering, which is easy to miss because it is stated in **three places, none of which is the crate itself**:

- `.mstar/specs/entity-scope-model.md` — nexus-spoke-adapter is "**the sole crate that directly depends on spoke-operations**"
- `.mstar/specs/spoke-adapter-architecture.md` — nexus-local-db = "**Pure storage** ... no spoke types or dep on spoke-adapter"
- `crates/nexus-local-db/AGENTS.md` — records V1.146 **removing** exactly this dep once before

## Guidance

When a spoke-op validator must gate a storage write:

1. **Storage crate stays pure**: raw column params (`&str`/`Option<&str>` JSON strings), no spoke imports, no validation.
2. **Gate fn lives in `nexus-spoke-adapter`** (e.g. `adapter/mind_state.rs::validate_and_store_mind_state`): validate first (reject → nothing persisted), then map validated JSON to raw params and delegate to the store.
3. Before adding a spoke dep to ANY crate, grep the two specs + the crate's AGENTS.md for the layering sentence — the sole-consumer rule has history (it was violated and reverted in V1.146 too).

## Why This Matters

The dep-reversal topology (V1.145: adapter depends on local-db, never the reverse) is what keeps spoke consumable at one boundary. A validate-gate in storage re-introduces the reversed edge invisibly — compilation is fine, tests pass, and the violation only surfaces in QC against spec text.

## When to Apply

- Any new spoke-op validator integration (validate_mind_state today; `applyMindDeltas` and future mind-axis ops if extracted under the ≥2-consumer gate).
- Code review of diffs touching `crates/*/Cargo.toml` + spoke.

## Examples

V1.164 P2 T2: initial a10d5e4e had the dep in nexus-local-db; fix 2d6f41d9 moved the gate to `nexus-spoke-adapter/src/adapter/mind_state.rs` (validate → map → store) with pure-storage CRUD kept in `mind_state_store.rs`. Rejection tests moved with the gate (adapter-side); pure CRUD tests stayed storage-side.

---
title: Codegen optional field callsite coverage
category: engineering
track: knowledge
source_plan: 2026-08-12-v1.163-p2-outbox-drop-and-flake-sweep
source_iteration: V1.163
created: 2026-08-12
last_updated: 2026-08-12
status: active
---

# Codegen Optional Field Callsite Coverage

## Context

When `pnpm run codegen` regenerates Rust types from JSON Schema (via `typify`), adding an optional field to a schema (e.g., `world_event_id?: string`) updates the generated Rust struct (`pub field: Option<String>`) but does **not** update existing struct-literal callsites that construct the type. Rust's `E0063` (missing field) only surfaces when the crate containing the callsite is compiled — which may be a different crate from the generated types.

## Guidance

After adding an optional field to a JSON Schema that feeds `typify` codegen:

1. **Run `pnpm run codegen`** to regenerate Rust + TS types.
2. **Run `SQLX_OFFLINE=true cargo check --workspace`** (not just the contracts crate) to catch `E0063` across all callsites immediately.
3. **Fix callsites**: add `field_name: None` to every struct literal (production + test).
4. **Run `./tooling/check-wire-drift.sh`** to verify schema ↔ generated type parity.
5. **Run `SQLX_OFFLINE=true cargo test --all`** to verify no runtime regressions.

The "test fixtures remain valid" claim (which is true for TS/JSON test data) is **false** for Rust struct literals — they must be updated explicitly.

## Why This Matters

A schema change in one crate (`schemas/` → `nexus-contracts`) can silently break compilation in a downstream crate (`nexus-daemon-runtime`) that wasn't part of the task's test scope. If the next task or plan doesn't compile the affected crate, the breakage propagates and blocks unrelated work (observed in V1.163: P1 schema change broke P2's `cargo test --all`).

## When to Apply

- Any `pnpm run codegen` after a JSON Schema optional field addition
- Any struct-literal construction of `typify`-generated Rust structs
- Especially when the schema-change task and the implementation task are different SDD tasks

## Examples

**V1.163 P1**: Added `world_event_id?: string` to `work-outline.schema.json`. Codegen updated `WorkOutlineTimelineEventsItem` in `nexus-contracts` (added `pub world_event_id: Option<String>`). But 5 struct-literal callsites in `crates/nexus-daemon-runtime/src/api/handlers/outline.rs` (1 production `timeline_add_event` + 4 tests) were not updated → `E0063` on `cargo check --workspace`. Fixed in P2 commit `d6af9859` with `world_event_id: None` at each site.

## Prevention

Consider a CI gate or post-codegen assertion that runs `cargo check --workspace` after `pnpm run codegen`. Tracked as R-V1163P2QC1-002.

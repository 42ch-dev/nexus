---
title: Codegen optional field callsite coverage
category: build-errors
track: knowledge
last_updated: 2026-10-12
created: 2026-08-12
status: active
problem_type: build_error
module: contracts-codegen
date: 2026-08-12
severity: medium
symptoms:
  - "Rust E0063 'missing field' errors appear in a crate whose struct-literal callsites were not updated by codegen"
  - "a schema-only change (added optional field) passes the contracts crate but breaks a downstream crate's `cargo check --workspace` / `cargo test --all`"
  - "the breakage surfaces later than the change, in an unrelated task's build"
root_cause: "typify regenerates the Rust struct with an Option<T> field but does not touch existing struct-literal callsites, so every literal constructing that type must be updated by hand; a task-scoped build that skips the downstream crate hides the breakage until it is compiled."
resolution_type: workflow_improvement
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

A schema change in one crate (`schemas/` → `nexus-contracts`) can silently break compilation in a downstream crate (`nexus-core`, then owned by `nexus-daemon-runtime` before its v1.193 retirement) that wasn't part of the task's test scope. If the next task or plan doesn't compile the affected crate, the breakage propagates and blocks unrelated work (observed in V1.163: P1 schema change broke P2's `cargo test --all`).

## When to Apply

- Any `pnpm run codegen` after a JSON Schema optional field addition
- Any struct-literal construction of `typify`-generated Rust structs
- Especially when the schema-change task and the implementation task are different SDD tasks

## Examples

**V1.163 P1**: Added `world_event_id?: string` to `work-outline.schema.json`. Codegen updated `WorkOutlineTimelineEventsItem` in `nexus-contracts` (added `pub world_event_id: Option<String>`). But 5 struct-literal callsites in `crates/nexus-core/src/outline.rs` (1 production `timeline_add_event` + 4 tests) were not updated → `E0063` on `cargo check --workspace`. Fixed in P2 commit `d6af9859` with `world_event_id: None` at each site.

**V1.169 P1** (typify optional-**array** collapse — semantic quirk, not a compile break): typify renders an optional JSON-Schema array member as `Vec<T>` + `serde(default)`, making **absent ≡ `[]` indistinguishable**. For PATCH DTOs where "not provided" (unchanged) must differ from an explicit empty array (meaningful clear), a plain optional array breaks the contract silently. Fix: `["array","null"]` union on the member → generated `Option<Vec<T>>` where absent/null = unchanged and `Some(vec![])` = clear. Documented in the update schema's description so the union is not "simplified" away later. Observed on `world-rule-update-request.schema.json` `target_entry_types` (AR-1/AR-3 deviation ratified in V1.169).

## Prevention

Consider a CI gate or post-codegen assertion that runs `cargo check --workspace` after `pnpm run codegen`. Tracked as R-V1163P2QC1-002.

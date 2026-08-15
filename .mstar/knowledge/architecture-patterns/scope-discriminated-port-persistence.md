---
module: nexus-spoke-adapter
date: 2026-08-15
problem_type: knowledge
category: architecture-patterns
severity: medium
plan_id: 2026-08-14-v1.165-p1-entry-scoped-findings-alignment
tags: [spoke, findings, extensions-discriminator, dual-table, world-scope, work-scope, routing]
last_updated: 2026-08-15
applies_when: Persisting spoke port outputs whose nexus home differs by scope (world vs work); extending FindingPort behavior; adding a second storage home behind one spoke port method; any plan touching finding_port.rs routing
---

# Scope-Discriminated Port Persistence (extensions.nexus Routing)

> Extends the adapter-boundary family: `spoke-adapter-port-orchestration-adoption.md` (Surface B), `spoke-op-gate-at-adapter-boundary.md` (layering). This doc covers **routing one spoke port output to multiple nexus storage homes by scope**.

## Context

spoke `FindingPort` is a **single-method** port (`put_findings(finding: Vec<Finding>)`) and `orchestrate_check` passes **no scope** to it — the port cannot ask "which world/work is this for?" So V1.164's world-scoped mental checker findings had nowhere to go: the only home was the work-scoped `findings` table (`work_id` NOT NULL FK → `works`), and worlds have no works row keyed by world id. Result: correct findings → `put_findings` → FK violation → HTTP 500.

## Guidance

Route **inside the adapter's port impl** using the `extensions.nexus` namespace as the scope discriminator:

- `extensions.nexus.work_id` present (no `world_id`) → legacy work path, **byte-identical** behavior
- `extensions.nexus.world_id` present (no `work_id`) → world path into the scope-aligned home (`world_findings`, FK → `narrative_worlds`)
- both or neither → `INVALID_INPUT` reject naming the entity id
- The discriminator rides the spoke wire type itself (ExtensionMap) — no port signature change, no spoke release needed
- **One batch transaction** across both tables (single SQLite file makes this free); mixed-scope batches commit or roll back atomically
- Producers stamp the discriminator (checker stamps `world_id = scope.scope_id`); producers must NOT stamp a fake `work_id` to satisfy a legacy FK

## Why This Matters

The rejected alternatives both fail structurally: a spoke port split needs a spoke release (pinned lockstep); handler-side routing would fork `orchestrate_*` sequencing. The extensions discriminator is the only place where scope is available without changing the protocol — and it generalizes to any future scope-split port output (relations, rules, mind-axis ops).

## When to Apply

- Any spoke port persistence whose nexus home is scope-dependent
- Reviewing diffs that add columns to a legacy table to "support both scopes" — check for whole-table enumerating consumers (global watchers, prunes, per-creator lists) before widening

## Examples

V1.165 P1: `finding_port.rs` discriminator + `world_findings` table (spoke vocabulary verbatim on the world path; work-path severity mapping unchanged). V1.164's bug was the checker mis-stamping the world id into `work_id` — the discriminator made the mis-stamp a reject instead of an FK 500.

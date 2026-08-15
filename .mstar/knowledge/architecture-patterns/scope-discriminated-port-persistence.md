---
module: nexus-spoke-adapter
date: 2026-08-15
problem_type: knowledge
category: architecture-patterns
severity: medium
plan_id: 2026-08-14-v1.165-p1-entry-scoped-findings-alignment
tags: [spoke, findings, extensions-discriminator, dual-table, world-scope, work-scope, routing, wrapper-seam, manifest-guard]
last_updated: 2026-08-15
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

## V1.166 Extension: Read-Side Wrappers Need the Same Discipline

When the scope constraint is on the **resolution/input side** (not the persistence side), the same boundary hosts a **wrapper around the spoke orchestrator** instead of port-impl routing. V1.166 `orchestrate_check_world_scoped` (`rule_query_port.rs`) world-scopes check rule resolution — spoke `RuleQueryPort::list_rules` has no world param and `resolve_check_rules` skips `list_rules` when refs are empty, so auto-include must be pre-expansion at the wrapper:

- Gates fire **before** delegation (reject evaluates nothing, persists nothing — fail-closed whole-operation)
- **Both production callers must cross the wrapper** (daemon `check.rs` + Connect N-C2 `invoke.rs`) — a handler-only gate leaves the other caller unguarded; the wrapper is the single choke point
- **The compile-time manifest guard must certify the REAL production entrypoint.** V1.166 QC caught (`manifest.rs` op-mapping proof still typechecking raw `orchestrate_check` after the wrapper landed): a stale guard stays green while a future caller reverts to the raw function — the exact seam-bypass drift the guard exists to catch. When a wrapper becomes the production path, update the guard arm in the same change.
- **Validation-gate placement follows wire topology:** carriers never arrive on the check wire (embedded rules hard-rejected at the wrapper), so a CLI-only carrier validation gate provably covers Connect too — no daemon-side revalidation needed. Derive gate location from what can appear on each wire, not from defensiveness.

## When to Apply

- Any spoke port persistence whose nexus home is scope-dependent
- Reviewing diffs that add columns to a legacy table to "support both scopes" — check for whole-table enumerating consumers (global watchers, prunes, per-creator lists) before widening
- Adding a wrapper around a spoke orchestrator for scope/isolation semantics — both production callers adopt it and the manifest compile guard certifies the wrapper, not the raw function

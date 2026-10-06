---
module: crates/nexus-core (execution/compute), nexus-spoke-adapter, apps/nexus-service error boundary
date: 2026-10-07
problem_type: api_design
category: api-design
severity: high
plan_id: 2026-10-07-v1.206-p2-typed-error-classification
applies_when:
  - designing or reviewing refusal shapes for resource lookups keyed by caller-supplied ids (run / schedule / relation / entry)
  - a differential between "row exists but not yours" and "row absent" is observable by the caller
  - unifying error shapes across an internal typed surface and an external wire boundary
tags:
  - error-surface
  - existence-opacity
  - refusal-shape
  - internal-typed-external-flat
  - run-id
  - relation-oracle
---

# Resource refusals: opaque outward, typed inward

## Context

v1.206 P2 converged the compute run-ID refusal surface (`R-V1195P2-001`: "Compute
run ID distinguishes foreign World from unknown row"), and the plan QC tri-review
caught the mirror-image defect in the first implementation (`F-001`: ownership
denial was folded into the flat `NotFound` **at the decision site**, erasing the
typed internal distinction the contract required). The same iteration closed the
relation-create existence oracle (`R9`) and documented its deliberate counterpair
(`R8`, an existence-confirming refusal on a create path).

The pattern now has three in-repo precedents: schedules identical-close
(`crates/nexus-core/tests/retained_execution_contracts.rs:3089-3095` — "an unknown
id and a foreign id must close with the same refusal"), workflow-runs 404 (v1.195
§4), and entity-scope 404-cross-world.

## Guidance

1. **External shape: one refusal for absent and not-yours.** Both close with the
   same request-derived shape (e.g. `404 NotFound { resource: "run {run_id} not
   found" }`) — no existence oracle, no owner-scope leak, no distinct status code.
2. **Internal shape: keep the typed distinction.** Ownership denial must stay a
   distinct typed outcome internally (v1.206 uses `RunVisibility { Visible(Box<row>)
   / Absent / ForeignDenied }`; the collapse to the flat refusal happens in the
   shared outward boundary function — `visible_run_or_hidden` — **after** the
   check, never inside it).
3. **Pin both properties.** An *opacity* pin (foreign and unknown produce the
   identical external shape on every route) **plus** a focused assertion for the
   internal distinction (visibility-helper states). A pin that only checks the
   external shape cannot catch a regression that erases the internal type; a pin
   that only checks the helper cannot catch an external leak.
4. **Check the counterpair discipline on create paths.** An existence-confirming
   refusal (e.g. `KnowledgeEntryAlreadyExists` from a hidden-row PK collision) may
   be intentional where the caller must pick a fresh id — but then document the
   tradeoff next to the site that *removes* the signal elsewhere (R9's
   phantom-success), so the two sides sit side by side with recorded reasons, not
   as an unexplained asymmetry.
5. **Fault reachability ≠ exit-site topology.** Typing a fault (e.g. adding a
   `details.category` for a bare reason) can move a fault out of a generic 500
   class without changing any exit site; census both when reviewing an error
   surface.

## Why This Matters

- **A boolean collapse at the check site destroys the contract silently.** The
  external behavior can look perfect (one opaque 404) while the internal typed
  contract is gone — the v1.206 QC finding was invisible to external-shape tests.
- **Existence oracles are security-adjacent, not cosmetic.** A foreign/unknown
  differential lets a caller probe which ids exist in scopes they do not own
  (`verify-stored-row-scope-before-cas-write.md` documents the write-path variant
  of the same family).
- **The reverse direction matters too.** R8 shows a create path where confirming
  existence is the *mandated* behavior; blanket "make everything opaque" would
  break id-selection flows. The discipline is a recorded per-site decision.

## When to Apply

- Designing/reviewing refusals for id-keyed lookups: compute runs, schedules,
  workflow runs, relations, knowledge entries.
- Migrating a refusal surface where internal typed variants exist.
- QC review of "unify to one 404" changes — ask specifically **where** the
  collapse happens.

## Examples

### Before (v1.206 F-001, refuted shape)

```rust
// decision site folds denial into the flat error — internal distinction lost
match ensure_run_visible(pool, principal, run).await {
    Ok(true)  => { /* proceed */ }
    Ok(false) => return Err(run_not_found(run_id)),   // foreign AND absent collapse HERE
    Err(e)    => return Err(e),
}
```

### After (v1.206 P2 shape)

```rust
// typed outcome internally; collapse only at the shared outward boundary
enum RunVisibility { Visible(Box<ComputeRun>), Absent, ForeignDenied }
// run_visibility() returns the typed state; visible_run_or_hidden() maps
// Absent | ForeignDenied -> run_not_found(run_id) for the caller.
```

## See also

- `architecture-patterns/verify-stored-row-scope-before-cas-write.md` — the
  write-path sibling (payload-claim vs stored-scope, OCC reject leaks).
- Spec amendment pending: `.mstar/specs/runtime/daemon-api-surface-conventions.md`
  §12.3 error-table entry; iteration draft
  `.mstar/iterations/v1.206/specs/compute-run-id-opacity.md` (Promoted here).
- Regression pins: `crates/nexus-core/tests/capability_compute.rs`
  `run_detail_and_foreign_run_refusals`; `nexus-spoke-adapter` relation
  absent-id-shape differential.

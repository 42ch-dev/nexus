# Workspace create end-to-end: core capability, service route, web flow

## Context

Iteration v1.220 (plan `v1220-workspace-create`) closed I-000063: workspace
creation was defined in contracts but missing in all three layers — no core
capability, no service route (a `route_not_migrated` placeholder), and a hidden
web affordance. The delivery crossed Rust core, TypeScript service, and React
web in one plan, with tri-QC driving three fix waves that converged on three
reusable decisions recorded here.

## Decisions that generalize

### 1. One derivation authority, one slug rule

`default_creative_root` in the core is the ONLY place that derives
`<root>/<slug>`; the web's `previewRoot` mirrors it through a shared slug rule:
ASCII names slugify; pure-CJK names fall back to `ws-<fnv1a32 hex>`. When a
preview and a core path must agree, give the derivation to the lower layer and
make the upper layer a display mirror — never fork a second rule "just for the
preview" (it drifts; qc3 flagged exactly that drift as a deferred residual).

### 2. Name-scoped preserve-on-omit for PATCH-style saves

A PUT that omits an optional field has two defensible meanings: "keep the
stored value" or "clear it". For agent-profile-style saves the resolution is
**name-scoped**: preserve the stored optional field only when the stored
record's identity key (the name) equals the incoming one; a different identity
with an omitted field stores without it. Blind preserve-on-omit leaks the old
record's field into the new one (a switch from agent A with a custom launch
command to agent B would inherit `--flag`). The client tracks field origin
(`stored`/`scan`/`edited`) and only sends fields the user actually edited;
everything else is omitted so the server's merge rule decides.

### 3. Serialize check-then-act transactions; roll back only own artifacts

Core `create_workspace` had a check→probe→materialize→register sequence with no
mutual exclusion; concurrent creates could both pass preflight, and the loser's
rollback (`remove_dir_all`) could destroy the winner's committed registration
(a 201 already returned to the client, then silently destroyed). Fix shape:

- a process-global async mutex taken BEFORE preflight and held through rollback;
- rollback that removes each artifact only if THIS call created it
  (preexisting dirs/db/manifest are never touched; adopted roots are never
  deleted).

Same-process serialization is enough for the single-daemon product model; note
the limit (cross-process still unserialized) next to the lock. Two concurrency
tests pin the contract: exactly one 201 for same-slug and same-root races, loser
gets the typed conflict, no loser artifacts remain.

### 4. Partial success is a first-class state, not an error

Create-then-activate spans two mutations. If create commits but activate fails,
treating it as total failure makes the UI hide the created record and pushes the
user into a guaranteed-409 retry that orphans workspaces. Instead: invalidate
the list immediately on create commit (the chip appears, inactive), move the
panel to a `createdNotActivated` state with a retry that re-runs ONLY the second
mutation, and phase-scope the error copy so create-failure text can never render
in the created state (QC caught the English "The workspace was not created"
leaking into the activation-failure state).

### 5. New typed errors must cross every hand-maintained fold

`CoreError::WorkspaceConflict` existed in Rust, but the wire-code set in
`packages/nexus-native/src/errors.ts` is hand-maintained; forgetting it degraded
the typed 409 into a generic 500. When adding a core error variant, grep the
fold chain end-to-end (napi → generated TS → native error map → service fold)
and add a route-inventory-level test that asserts the status code, not just the
variant.

## Process lesson: dead-code audits need cross-package eyes

The iteration's repository-wide dead-code audit deleted modules whose only
importers lived in another app (`apps/design-studio` consuming `apps/web` via
`@web-layout/*` / `@web-settings/*` / `@web-setup/*` / `@web-shell/*` aliases);
`apps/web` typecheck stayed green and the breakage only surfaced in
`pnpm --filter design-studio typecheck`. The audit method now requires: alias
grep across consuming packages, basename grep outside the owning tree, and a
typecheck of every consumer app before any deletion. Reusable record:
`.mstar/iterations/v1.220-developer-shell/guides/dead-code-audit.md`.

## Pointers

- Live code: `crates/nexus-core/src/home.rs` (`create_workspace`, `CREATE_LOCK`),
  `apps/nexus-service/src/workspaces.ts`, `apps/web/src/components/shell/workspace-create-panel.tsx`.
- Companion spec (promoted): `specs/active/architecture/agent-profile-service-persistence.md`
  (the name-scoped preserve-on-omit rule, §2 Merge semantics).
- QC record: `.mstar/sdd/v1220-workspace-create/review/qc-consolidated.md`.

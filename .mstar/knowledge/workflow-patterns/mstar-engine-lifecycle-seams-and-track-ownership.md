---
module: Morning Star engine lifecycle seams (snapshot phase machine, leases, track ownership, cleanup)
date: 2026-09-25
problem_type: workflow_issue
category: workflow-patterns
severity: high
plan_id: 2026-09-25-v1.197-p1-worktree-submodule-gitdir
applies_when:
  - driving an iteration from Phase 1 to close with the installed CLI
  - a lifecycle verb refuses with not-prepare / not-pending / stale-branch-claim / foreign-* codes
  - recording an integration checkout, plan parallelism, leases or a plan row's Done transition
  - preparing worktree or branch cleanup for merged plans
tags:
  - morning-star
  - engine-seams
  - snapshot
  - lease
  - ownership
  - worktree-cleanup
  - phase-machine
---

# Engine lifecycle seams: declare the phase you are in, register the tracks you create

## Context

The installed harness CLI owns a small set of coordinated writes (snapshot, root register, project register) behind a same-host lock. Several of its behaviours are not obvious from the skills, and each one costs an iteration's worth of confusion the first time it is hit. This doc records the seams that were actually exercised and measured on 2026-09-25 (CLI as installed), so a future coordinator does not have to rediscover them.

## Guidance

### The snapshot's `phase` is a coordinator-written label — the CLI will not write it for you

- `mstar iteration register` creates the snapshot **without** `phase`, so `mstar workflow show-prepare` reports `allowed:false` / `not-prepare` and `mstar workflow amend-prepare` refuses.
- The supported writer is the **coordinated snapshot replacement**, which permits only `phase` and `updated_at` to differ from disk:

  ```sh
  mstar-harness workflow show-prepare --session <coordinator.json> --json      # current snapshot+compass versions
  jq '. + {phase: "phase-1-prepare"}' snapshot.json > payload.json             # phase-only delta
  mstar-harness persist snapshot --key <workflow-id> --expect-version <sha256> \
      --session <coordinator.json> --file payload.json
  ```

  Any other field drift is refused (`coordination.direct-write-refused`), so this is a phase/label writer, never a row editor. Advance it the same way (`phase-2-execute`, …) at each phase entry; `mstar_phase2 … bind` and the host hooks require the matching label.
- `amend-prepare` then records what Prepare owes: `mainWorktreeBranch`, `appendPlans` (an empty array is **required** by the schema), `integrationWorktreePath` and `planParallelism`. Record the integration checkout **after** creating it, because validation expects it to exist on its recorded branch.

### A cancelled model handoff is terminal — verify readiness facts yourself

`mstar_model_handoff {operation:"phase1-complete"}` refuses with `not-pending` when the earlier `direction-lock` binding was cancelled (for example by an unowned model change) — a terminal binding is never re-armed for that workflow. The phase-1-lock gate is therefore informational for **this** session only: the substantive readiness facts (three specialist returns, `compass status: locked`, PM-confirmed Prepare gates, a distinct integration checkout, remote tip == integration HEAD) still have to be true, and they can be verified directly. Model fallback is allowed; do not retry the handoff in a loop, and do not read the refusal as "Phase 1 failed".

### The engine's "dirty" includes ignored outputs, and its cleanup needs your bookkeeping

Two measured interactions between the lifecycle and reclamation:

- **Ignored-only footprints read as dirty.** A checkout whose tracked tree is clean but which holds ignored build outputs is refused as `cleanup.refuse.dirty-worktree`. Do not scaffold builds inside a reviewed checkout when avoidable; when it happens, enumerate the ignored paths/sizes as evidence and reclaim through the documented exact-path non-force route rather than treating the refusal as a defect.
- **Every track branch you create must be registered, or cleanup refuses it.** The plan row's `metadata.track_branches` is the ownership record the engine and the sweeper read. A branch created without being recorded (an extra fix-round branch, for example) produces `cleanup.refuse... / stale-branch-claim` — the guard is right, the bookkeeping was missing. Register the branch in the same round you create the worktree, not when you clean up.
- On the plan row's `Done` transition, the ownership producer obligation is to persist `metadata.working_branch`, `metadata.worktree_path` (and keep `track_branches`) **in the same locked update** that sets `Done` and deletes `execution_lease`; only then can the engine plan branch/worktree removal from the retained metadata.

### Row `Done` comes after the serial integration merge, never before it

Order that satisfies both the engine and the reviewers: claim `integration_merge_lease` → `git merge --no-ff <plan-branch>` inside the integration worktree → release the merge lease, set the row `Done`, delete `execution_lease`, persist the ownership metadata — all in one locked update. Only the merge-lease holder merges, and merges stay serial regardless of how many tracks ran in parallel.

### Let the artefact certify the end state

Convergence ("only the main checkout and the integration worktree remain") was signed off by running the repository's own sweeper (`--check-convergence` exit 0 with no reasons) and a per-track `--check-exit`, after the engine had deleted every merged branch. Prefer a machine-checkable artefact over a coordinator's summary — especially for claims a reviewer will later have to trust.

## Why This Matters

Each seam above fails **closed**: the verb refuses rather than half-writing, which is the right behaviour but looks like a blocker if you do not know the writer. Knowing them turns three apparent dead ends (unwritable phase, terminal handoff, refused cleanup) into routine steps, and keeps the important invariant intact — process state changes only through coordinated writes with ownership recorded at creation time.

## When to Apply

- Any iteration driven through the installed CLI from a coordinator session (Phase 1 lock through Phase 6 close).
- Whenever a lifecycle verb refuses with `not-prepare`, `not-pending`, `stale-branch-claim`, `foreign-worktree`/`foreign-branch` or `dirty-worktree`.
- Before planning worktree/branch cleanup for a merged plan.

## Examples

```sh
# phase label (only phase/updated_at may differ from disk)
jq '. + {phase: "phase-2-execute"}' snapshot.json > payload.json
mstar-harness persist snapshot --key v1.197 --expect-version "$VER" --session "$SESS" --file payload.json

# Prepare owes the integration checkout + parallelism (appendPlans is required, even empty)
printf '%s' '{"mainWorktreeBranch":"main","appendPlans":[],"integrationWorktreePath":"/abs/.worktrees/iteration-v1.197","planParallelism":"parallel"}' > amend.json
mstar-harness workflow amend-prepare --session "$SESS" --expect-snapshot "$SNAP_V" --expect-compass "$COMPASS_V" --input amend.json

# cleanup refuses unregistered track branches — record them at creation, then let the engine delete
mstar-harness worktree cleanup --workflow v1.197 --harness <harness>          # dry-run
mstar-harness worktree cleanup --workflow v1.197 --harness <harness> --apply
```

The row-level Done authority and the ownership obligation are summarised in the plan headers of this iteration; the worktree/branch side is in the worktree-lifecycle doc.

---
module: Morning Star harness iteration entry and residual register (repo operating knowledge)
date: 2026-09-21
problem_type: workflow_issue
category: workflow-patterns
severity: medium
plan_id: 2026-09-21-v1.194-p2-retirement-residuals
applies_when:
  - Entering a formal iteration and a model-handoff or phase checkpoint refuses
  - A shell/tool surface does not receive the injected session environment variable
  - Claiming execution leases, or binding Phase 2, from a session whose identity the engine did not mint
  - Reading or writing the residual register when entries belong to plans that no longer run
tags:
  - mstar-harness
  - host-binding
  - coordinator-identity
  - session-id
  - execution-lease
  - phase-2-entry
  - residual-register
  - retired-plan-bucket
---

# Host identity vs engine identity, and residuals in retired plan buckets

## Context

Entering a formal iteration in this repo follows a fixed chain: direction lock → ordered specialist returns (product → architecture → writing) → PM Prepare acceptance → integration-branch publication → a coordinator **model-handoff** checkpoint → Phase-2 observation bind → per-plan execution leases → per-plan SDD implementation, plan QC tri-review and QA → iteration close → PR delivery.

Two independent identities exist along that chain, and only one of them is involved in most of it:

| Identity | Minted / armed by | Consumed by |
|---|---|---|
| Host session binding | the host adapter, when the session is armed | the model-handoff checkpoint, which requires the recorded coordinator id **and** the envelope session id to equal the armed host binding id |
| Engine coordinator session | the engine's coordinator `bind` (a fresh id per bind) | every plan/workflow verb: workflow snapshot phases, plan envelopes, Phase-2 observation, execution leases, register writes |

A formal iteration hit the seam between them. Prepare content, the three ordered returns, the PM lock and the integration publication were complete, and the workflow rows were still `Todo` — but the required Phase-1 completion checkpoint refused with `binding-invalid`, leaving the handoff pending and no phase change recorded. The coordinator bind had returned an engine-generated session id of its own; the shell tool surface exposed no `MSTAR_HOST_SESSION_ID` (the variable was absent, `printenv` exited 1, and an unrelated marker variable confirmed that the surface receives no injected environment at all). The installed host adapter injects the variable only for a tool event whose name matches its expected shell tool, by adding an `env` field to that tool's input — and the surface in use had no such field. The documented CLI exposes `bind` / `resume` and scoped lifecycle verbs but **no** coordinator-identity reassociation verb; `bind --resume` is explicitly read-only. No supported path could satisfy the checkpoint at that moment, and no envelope, ledger, ownership block or plugin check was edited to fake one.

What mattered next was establishing the **boundary** of the failure rather than treating it as a stopped iteration. Only the model-handoff checkpoint consumes the host identity; every path operating on the coordinator envelope still worked:

- the workflow snapshot's phase projection advanced to `phase-2-execute` through the versioned coordinator-authorised writer (only the phase and `updated_at` fields are writable there; the pre-write byte version is recorded as a hash);
- the Phase-2 observation bind succeeded;
- per-plan execution leases were claimed with explicit session ids, and task briefs for both wave-1 plans were generated.

The pending handoff was then cancelled **by the host itself** when an unowned model change was recorded, which made the checkpoint moot for that iteration: its only purpose was the coordinator model switch, and the user switched model manually. The true host session id was read from the host tool rather than inferred (`mstar_phase2 bind` reported both the host session id and the coordinator session id), and the earlier "Phase 2 is blocked" statement was withdrawn in writing once the engine paths were measured. The identity divergence itself is a known upstream bootstrap defect (tracked upstream at <https://github.com/btspoony/mstar-harness/issues/273>), and this repository owns product code, not the harness implementation — so the operating answer here is discipline, not a patch.

## Guidance — running the entry with divergent identities

1. **Two identities, two scopes.** Before concluding "the engine is unavailable", establish *which* identity the refusing check consumes. A host-binding refusal says nothing about the engine verbs, and vice versa; measure the other side before writing a blocker that stops the iteration.
2. **Never derive a session id from the filesystem.** Filesystem recency, a lock file, or a `history://`-style artifact are not the armed binding; an engine-minted coordinator id is not the host id either. Read the host identity from the host tool that reports it.
3. **Pass an explicit session id on every bind and lease claim.** When the tool surface does not receive the injected variable, the environment is simply absent for that command — supply the id explicitly and record which id each claim used.
4. **Distinct rows need distinct explicit ids.** The engine keys one plan-PM envelope per session id, so a second fresh bind with the same id refuses (`coordination.session-mismatch`). A per-plan suffixed id derived from the session's own host id is the traceable form.
5. **Treat a checkpoint refusal as scoped, and log what still worked with its own evidence.** Phase-projection advance, observation bind and lease claims are separately observable; a blocker note that asserts the phase is blocked without measuring them will have to be withdrawn.
6. **Never falsify a session envelope, reset ownership, or hand-edit a binding ledger to pass a checkpoint.** When the checkpoint's purpose is satisfied through a supported path (here: the host cancelled the handoff and the model switch happened manually), the cancelled checkpoint is a legitimate terminal state — record it plainly, and correct the earlier blocker text so the two states do not both stand.
7. **Keep the residual writes the engine's job.** Register entries are written through engine writers; the register's closure state is not something to reconcile by hand because the product state looks settled.

## Guidance — residuals that live in retired plan buckets

The project's residual register is bucketed **per plan**. The current iteration's deferrals belong in its own bucket and close normally. Entries inherited from a plan that has already been retired are different: no exposed writer can close them.

Observed refusals from a plan that fixed another plan's entries and tried to record the closures:

| Attempt | Result |
|---|---|
| `mstar plan residual-close --entry <inherited-id>` from the current plan's session | exit 1 — the entry "is not registered on plan &lt;current plan&gt;" (the verb is scoped to the calling plan's own bucket) |
| versioned register write, without `--session` | exit 1 — current buckets "belong to coordinated plans — use residual-add/residual-close" |
| the same write with `--session <coordinator envelope>` | exit 2 — "session applies to a coordinated snapshot replacement only" |
| opening the retired plan's own session | not possible — retired plans have no session |

Consequences to state honestly:

- A verified fix for an inherited entry is recorded as a **carry-forward entry in the current plan's bucket**, carrying the per-ID evidence; the inherited entry stays `open`.
- The register's closure state and the product's actual state can therefore diverge for retired buckets. Name the divergence in the plan and iteration summary, so a later reader does not read `open` as "unfixed".
- Do not claim a closure the engine cannot record, and do not improvise one by editing the register or forging a plan session. A supported closure/re-homing path for retired buckets is ranked forward work (with an owner and a trigger), not a workaround.

## Why This Matters

- **A bootstrap identity failure misread as "engine unavailable" stops an iteration that can run.** The expensive error is not the refusal; it is generalising it. Here the refusal came from one checkpoint while the phase projection, the observation bind and both leases were available.
- **An unclosable bucket turns verified work into permanently open entries.** The register is the durable record other iterations plan against; if it cannot be closed, the *narrative* around it must carry the truth, or forward planning will re-queue already-fixed work.
- **Improvised fixes destroy the audit trail.** Hand-edited ledgers, forged session ids or forced takeover make every later claim about ownership and closure unverifiable — a cost that only appears on the next unowned/blocked workflow.
- **Blocker notes must be as corrigible as code.** Two contradictory statements ("blocked" / "entered") in the same iteration summary are worse than either alone; the correction is part of the evidence.

## When to Apply

- Entering a formal iteration when a model-handoff or phase checkpoint refuses, or when a bind returns an identity you did not expect.
- Any session where the shell/tool surface has no session environment variable and the next step needs one.
- Writing or reading a plan/iteration residual summary: check the bucket of every entry before treating `open`/closed as current product state.
- Deciding whether an inherited residual needs a carry-forward entry (it does) versus a closure (not available).

## Evidence

- Entry refusal: the Phase-1 completion checkpoint returned `binding-invalid`; the workflow rows remained `Todo` with no leases and no product commits at that point — Prepare artifacts and the integration branch were already published.
- Boundary measurement: workflow snapshot phase projection advanced to `phase-2-execute` through the versioned coordinator-authorised writer (pre-write byte version hash recorded); Phase-2 observation bind returned success; per-plan leases claimed for both wave-1 plans with explicit ids and their worktree/branch pair.
- Host identity: reported by the host tool as the session id it had actually armed, together with the engine coordinator id, making the divergence explicit rather than assumed; the handoff was cancelled by the host on an unowned model change, and the model switch was performed manually.
- Register limitation: the three refusals above, with their exit codes, recorded when a plan whose own fixes were verified tried to close entries belonging to retired plans; the same limitation appears in that plan's summary and in two sibling plans' summaries referencing the same carry-forward entry.

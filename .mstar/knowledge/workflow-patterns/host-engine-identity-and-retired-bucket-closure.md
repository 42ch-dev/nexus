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
last_updated: 2026-10-16
---

# Host identity vs engine identity, and residuals in retired plan buckets

## Context

Entering a formal iteration in this repo follows a fixed chain: direction lock → ordered specialist returns (product → architecture → writing) → PM Prepare acceptance → integration-branch publication → a coordinator **model-handoff** checkpoint → Phase-2 observation bind → per-plan execution leases → per-plan SDD implementation, plan QC tri-review and QA → iteration close → PR delivery.

Two independent identities exist along that chain, and only one of them is involved in most of it:

| Identity | Minted / armed by | Consumed by |
|---|---|---|
| Host session binding | the host adapter, when the session is armed | the model-handoff checkpoint, which requires the recorded coordinator id **and** the envelope session id to equal the armed host binding id |
| Engine coordinator session | the engine's coordinator `bind` (a fresh id per bind) | every plan/workflow verb: workflow snapshot phases, plan envelopes, Phase-2 observation, execution leases, register writes |

A session can hit the seam between them: the coordinator `bind` returns an engine-generated session id of its own, while the shell/tool surface exposes no `MSTAR_HOST_SESSION_ID` (the variable is absent and `printenv` exits 1). The installed host adapter injects that variable only for a tool event whose name matches its expected shell tool, by adding an `env` field to that tool's input — a surface with no such field receives nothing. The documented CLI exposes `bind` / `resume` and scoped lifecycle verbs but **no** coordinator-identity reassociation verb, and `bind --resume` is explicitly read-only.

When this happens, establish the **boundary** of the failure rather than treating the iteration as stopped. Only the model-handoff checkpoint consumes the host identity; every path operating on the coordinator envelope still works:

- the workflow snapshot's phase projection advances through the versioned coordinator-authorised writer (only the phase and `updated_at` fields are writable there; the pre-write byte version is recorded as a hash);
- the Phase-2 observation bind succeeds;
- per-plan execution leases can be claimed with explicit session ids.

Never satisfy the checkpoint by editing an envelope, ledger, ownership block or plugin check. The identity divergence is a known upstream bootstrap defect (tracked upstream at <https://github.com/btspoony/mstar-harness/issues/273>); this repository owns product code, not the harness implementation — so the operating answer is discipline, not a patch.

## Guidance — running the entry with divergent identities

1. **Two identities, two scopes.** Before concluding "the engine is unavailable", establish *which* identity the refusing check consumes. A host-binding refusal says nothing about the engine verbs, and vice versa; measure the other side before writing a blocker that stops the iteration.
2. **Never derive a session id from the filesystem.** Filesystem recency, a lock file, or a `history://`-style artifact are not the armed binding; an engine-minted coordinator id is not the host id either. Read the host identity from the host tool that reports it.
3. **Pass an explicit session id on every bind and lease claim.** When the tool surface does not receive the injected variable, the environment is simply absent for that command — supply the id explicitly and record which id each claim used.
4. **Distinct rows need distinct explicit ids.** The engine keys one plan-PM envelope per session id, so a second fresh bind with the same id refuses (`coordination.session-mismatch`). A per-plan suffixed id derived from the session's own host id is the traceable form.
5. **Treat a checkpoint refusal as scoped, and log what still worked with its own evidence.** Phase-projection advance, observation bind and lease claims are separately observable; a blocker note that asserts the phase is blocked without measuring them will have to be withdrawn.
6. **Never falsify a session envelope, reset ownership, or hand-edit a binding ledger to pass a checkpoint.** When the checkpoint's purpose is satisfied through a supported path (e.g. the host cancels the handoff), the cancelled checkpoint is a legitimate terminal state — record it plainly, and correct the earlier blocker text so the two states do not both stand.
7. **Keep the residual writes the engine's job.** Register entries are written through engine writers; the register's closure state is not something to reconcile by hand because the product state looks settled.

## Guidance — residuals that live in retired plan buckets

The project's residual register is bucketed **per plan**. The current iteration's deferrals belong in its own bucket and close normally. Entries inherited from a plan that has already been retired are different: no **plan-session** writer can close them. **Supported closure:** the legacy bucket **can** be closed in place through `mstar status backlog-close --project <id> --key <plan-key> --id <R#> --note "<evidence>"`; evidence rides in the note (this face has no `closure_evidence` field). Note also that the `persist residuals` face is refused while the register contains any bucket owned by an active coordinated plan (`use residual-add/residual-close`).

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

- **A bootstrap identity failure misread as "engine unavailable" stops an iteration that can run.** The expensive error is not the refusal; it is generalising it. The refusal typically comes from one checkpoint while the phase projection, the observation bind and the lease claims are all available.
- **An unclosable bucket turns verified work into permanently open entries.** The register is the durable record other iterations plan against; if it cannot be closed, the *narrative* around it must carry the truth, or forward planning will re-queue already-fixed work.
- **Improvised fixes destroy the audit trail.** Hand-edited ledgers, forged session ids or forced takeover make every later claim about ownership and closure unverifiable — a cost that only appears on the next unowned/blocked workflow.
- **Blocker notes must be as corrigible as code.** Two contradictory statements ("blocked" / "entered") in the same iteration summary are worse than either alone; the correction is part of the evidence.

## When to Apply

- Entering a formal iteration when a model-handoff or phase checkpoint refuses, or when a bind returns an identity you did not expect.
- Any session where the shell/tool surface has no session environment variable and the next step needs one.
- Writing or reading a plan/iteration residual summary: check the bucket of every entry before treating `open`/closed as current product state.
- Deciding whether an inherited residual needs a carry-forward entry (it does) versus a closure (not available).

## Guidance — 2026-10-16 store cutover: register closure is now a store-issue PM action

The 2026-10-16 store cutover retired the file registers (`.mstar/projects/**/residuals.json`
moved under `archived/`) and the `mstar status backlog-close` verb (`status.verb-retired`).
Plans authored before the cutover still carry "register close-out" tasks whose Steps point at
the dead mechanism; treat them like any stale-target task under verify-first:

- The sanctioned successor is `mstar issue close` (unscoped) / `mstar plan issue-close`
  (plan-scoped). Closure is a **PM-authority action**: a leaf seat that hits the retired verb
  stops, records per-row verify-first evidence, and hands ready-to-run `ClosureEvidence`
  payloads to the PM instead of forcing a coordinator bind it must not have.
- `ClosureEvidence` shape: `reason` (required), `references` (required for `resolved`),
  `alignmentRef` (required for `resolved`/`waived`), `canonicalIssueId` (required for
  `duplicate`/`superseded`), `scope` (required for `waived`). CLI takes `--file <absolute
  json>` when the payload is not inline. `--expect` is the issue revision from
  `mstar issue show --id <id>` taken immediately before the write.
- Row-to-issue mapping lives in each issue's `occurrences[].occurrenceKey =
  register-entry:<batch>/<R-key>`.
- Closing on partial evidence is recoverable but embarrassing: close only what the evidence
  covers and reopen immediately when a half was missed (`mstar issue reopen --payload
  {"reason": ...}`), keeping the closed-half evidence in the reopen reason. A resumed
  iteration must re-read resume-point pending decisions before consolidating QC verdicts —
  a "keep open until X" ruling outranks an implementer's closure payload.
- Coordinator recovery after the creating session dies: `mstar-harness session recover
  --workflow <id> --prior-session <id>` (or `--unowned` only when the DB records none) with
  `--expect <workflow token from mstar status validate>` and an ActivationAttestation file:
  `{version: 1, attestedAt, operator:{actor, authorizationRef}, consumers:[{entryId, kind,
  entrypoint, runtime:"node", runtimeVersion, version, current, disposition}],
  stoppedSessions:[{sessionId, host, state}]}` — exactly one `current: true` consumer with
  `kind: "coordinator"`; dispositions: `reloaded`, `upgraded`,
  `excluded:not-this-control-root`, `excluded:no-store-access`, `excluded:superseded-binary`.

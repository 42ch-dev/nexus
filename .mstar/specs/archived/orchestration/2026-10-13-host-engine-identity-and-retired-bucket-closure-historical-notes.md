# host engine identity and retired bucket closure — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/workflow-patterns/host-engine-identity-and-retired-bucket-closure.md`](../../../knowledge/workflow-patterns/host-engine-identity-and-retired-bucket-closure.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 38

```markdown
A formal iteration hit the seam between them. Prepare content, the three ordered returns, the PM lock and the integration publication were complete, and the workflow rows were still `Todo` — but the required Phase-1 completion checkpoint refused with `binding-invalid`, leaving the handoff pending and no phase change recorded. The coordinator bind had returned an engine-generated session id of its own; the shell tool surface exposed no `MSTAR_HOST_SESSION_ID` (the variable was absent, `printenv` exited 1, and an unrelated marker variable confirmed that the surface receives no injected environment at all). The installed host adapter injects the variable only for a tool event whose name matches its expected shell tool, by adding an `env` field to that tool's input — and the surface in use had no such field. The documented CLI exposes `bind` / `resume` and scoped lifecycle verbs but **no** coordinator-identity reassociation verb; `bind --resume` is explicitly read-only. No supported path could satisfy the checkpoint at that moment, and no envelope, ledger, ownership block or plugin check was edited to fake one.
```

### at pre-freeze line 40

```markdown
What mattered next was establishing the **boundary** of the failure rather than treating it as a stopped iteration. Only the model-handoff checkpoint consumes the host identity; every path operating on the coordinator envelope still worked:
```

### at pre-freeze line 42

```markdown
- the workflow snapshot's phase projection advanced to `phase-2-execute` through the versioned coordinator-authorised writer (only the phase and `updated_at` fields are writable there; the pre-write byte version is recorded as a hash);
- the Phase-2 observation bind succeeded;
- per-plan execution leases were claimed with explicit session ids, and task briefs for both wave-1 plans were generated.
```

### at pre-freeze line 46

```markdown
The pending handoff was then cancelled **by the host itself** when an unowned model change was recorded, which made the checkpoint moot for that iteration: its only purpose was the coordinator model switch, and the user switched model manually. The true host session id was read from the host tool rather than inferred (`mstar_phase2 bind` reported both the host session id and the coordinator session id), and the earlier "Phase 2 is blocked" statement was withdrawn in writing once the engine paths were measured. The identity divergence itself is a known upstream bootstrap defect (tracked upstream at <https://github.com/btspoony/mstar-harness/issues/273>), and this repository owns product code, not the harness implementation — so the operating answer here is discipline, not a patch.
```

### at pre-freeze line 55

```markdown
6. **Never falsify a session envelope, reset ownership, or hand-edit a binding ledger to pass a checkpoint.** When the checkpoint's purpose is satisfied through a supported path (here: the host cancelled the handoff and the model switch happened manually), the cancelled checkpoint is a legitimate terminal state — record it plainly, and correct the earlier blocker text so the two states do not both stand.
```

### at pre-freeze line 60

```markdown
The project's residual register is bucketed **per plan**. The current iteration's deferrals belong in its own bucket and close normally. Entries inherited from a plan that has already been retired are different: no **plan-session** writer can close them. **v1.210 update (2026-10-10):** the legacy bucket **can** be closed in place through `mstar status backlog-close --project <id> --key <plan-key> --id <R#> --note "<evidence>"` — verified on the v1.208 sweep bucket while v1.210 was active; evidence rides in the note (this face has no `closure_evidence` field; `R-QC2-TOOLING-001`). Note also that the `persist residuals` face is refused while the register contains any bucket owned by an active coordinated plan (`use residual-add/residual-close`).
```

### at pre-freeze line 79

```markdown
- **A bootstrap identity failure misread as "engine unavailable" stops an iteration that can run.** The expensive error is not the refusal; it is generalising it. Here the refusal came from one checkpoint while the phase projection, the observation bind and both leases were available.
```

### at pre-freeze line 90

```markdown

## Evidence

- Entry refusal: the Phase-1 completion checkpoint returned `binding-invalid`; the workflow rows remained `Todo` with no leases and no product commits at that point — Prepare artifacts and the integration branch were already published.
- Boundary measurement: workflow snapshot phase projection advanced to `phase-2-execute` through the versioned coordinator-authorised writer (pre-write byte version hash recorded); Phase-2 observation bind returned success; per-plan leases claimed for both wave-1 plans with explicit ids and their worktree/branch pair.
- Host identity: reported by the host tool as the session id it had actually armed, together with the engine coordinator id, making the divergence explicit rather than assumed; the handoff was cancelled by the host on an unowned model change, and the model switch was performed manually.
- Register limitation: the three refusals above, with their exit codes, recorded when a plan whose own fixes were verified tried to close entries belonging to retired plans; the same limitation appears in that plan's summary and in two sibling plans' summaries referencing the same carry-forward entry.
```

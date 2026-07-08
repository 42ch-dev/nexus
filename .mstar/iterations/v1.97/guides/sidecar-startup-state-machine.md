# V1.97 Sidecar Startup State Machine Handoff

> **Promoted to:** `.mstar/knowledge/architecture-patterns/daemon-ready-gate-pattern.md` (V1.97 refinements, Rules 9-12). This file is an iteration-scoped snapshot retained for V1.97 history; durable invariants live in the knowledge doc + `specs/desktop-shell.md` §13.8.

## Purpose

This guide is the iteration-scoped architecture handoff for V1.97 Desktop First-Launch Reliability Hardening. Durable product contracts live in `specs/desktop-shell.md` §13.8 and `specs/web-ui.md` §29.12; this file captures implementation and QC detail for the active iteration. Do not copy it into `knowledge/` during iteration-start; the iteration-close compound pass decides whether any lesson becomes durable knowledge.

## State Machine Invariants

- A newly constructed `SidecarManager` starts with no owned child and state `Stopped`.
- `Starting` is a transient operational state. It is valid only while a spawn attempt is in progress or while an already owned child is being health-probed.
- `start_with_budget` may short-circuit an existing `Starting` state only when `inner.child.is_some()`. `Starting` with no child is invalid and must not block a real spawn attempt.
- Attaching to a healthy daemon on the resolved port is allowed, but attach does not create ownership. The manager may report the daemon as running/healthy, but stop/quit cleanup must only terminate a child the desktop app actually spawned.
- `Stopped` and `Error` are retryable. Retry/Reset actions must be able to attempt attach/spawn again or surface a bounded error with diagnostic detail.
- V1.97 uses the existing desktop status/detail path. It must not add daemon API routes, schema fields, or generated contract changes.

## Prototype Intake Rule

The current uncommitted product diff is evidence, not implementation. The SDD implementer must create an intake ledger in the task report that classifies each relevant prototype hunk:

| Classification | Required evidence |
|---|---|
| Accepted | Why the hunk satisfies a V1.97 task and which test/smoke evidence owns it |
| Revised | What changed from the prototype and why the replacement is safer |
| Rejected | Why it is out of scope, incorrect, or superseded, and confirmation it is not carried forward |

No hunk is accepted merely because it is already present in the working tree or because a prototype test happened to pass.

## Smoke Evidence Boundary

V1.97 requires two desktop smoke paths before Done:

- **Clean-state smoke:** use a cleared `~/.nexus42` or equivalent isolated home/profile. Record the command/environment, initial home/config state, wizard path, sidecar/UI state transitions, and final success or bounded recovery state.
- **Existing-install smoke:** use a pre-existing config/workspace. Record the preserved setup marker, workspace path behavior, daemon diagnostics, and final daemon/UI state.

The exact host mechanics are intentionally not specified here. A manual transcript, scripted local run, or browser/desktop automation can satisfy the gate if the evidence is reproducible enough for QC/QA. Unit tests are required supporting evidence for touched code, but they cannot replace either smoke path.


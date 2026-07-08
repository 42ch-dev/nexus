---
iteration_id: V1.97
start_date: 2026-07-07
end_date: 2026-07-08
status: completed
iteration_base_branch: main
target_branch: main
plans:
  - 2026-07-07-v1.97-desktop-first-launch-hardening
---

# V1.97 — Desktop First-Launch Reliability Hardening — Delivery Compass v1

## Scope

This iteration hardens the desktop first-launch path after the V1.96 manual smoke follow-up exposed that a new author can still get stuck before reaching Nexus. The product promise is simple: double-click the desktop app, choose or keep a workspace, and either enter the app or see a bounded, actionable recovery state. It is a reliability iteration, not a new product-surface iteration.

- **SP-1: First-launch setup wizard hardening.** Fix the current clean-state onboarding regressions that block author trust: cramped welcome layout, right-edge/flex overflow, directory picker argument casing, and any local-machine fixture leakage found during cleanup.
- **SP-2: Sidecar lifecycle correctness.** Ensure the desktop sidecar manager starts from an inactive state and does not suppress daemon spawn when no child process exists; `Starting` must mean an owned child is present or a spawn attempt is in progress, not a synthetic default. Preserve the bounded health-probe behavior and diagnostics from V1.96.
- **SP-3: Prototype-to-SDD handoff.** Treat the already-present uncommitted product diff as a pre-iteration prototype only. The V1.97 implementer must inspect, own, revise, or reject each relevant hunk and verify it under the SDD plan before any hunk can count as implementation.
- **SP-4: Hard smoke gate.** A clean `~/.nexus42` launch and an existing-install launch are both hard Done gates. Automated unit tests are required supporting evidence, but they cannot replace desktop smoke transcripts or an explicit hard-gate blocker.

## Grill-Me Decisions

| Decision | Resolution |
|---|---|
| Iteration direction | Broader **Desktop First-Launch Reliability Hardening**, not a narrow hotfix and not the old low-priority residual sweep. |
| Scope boundary | Include current P0 fixes, clean-state sidecar lifecycle, wizard recovery, and residuals that directly intersect the touched first-launch surfaces. Exclude signing, notarization, auto-update, multi-OS release hardening, broad config/db atomicity, and default-path consolidation. |
| Plan shape | Single SDD hardening plan. |
| Existing dirty diff policy | Existing uncommitted code is prototype/input only. `fullstack-dev` must take over, validate, revise or reject each relevant hunk, and record evidence before implementation is considered complete. |
| Local path cleanup | Remove `/Users/bibi` from current prototype/test fixtures. Historical QC/QA review-cwd evidence is not rewritten. |
| Branch policy | `iteration_base_branch=main @ 070e26f7`; `spec_integration_branch=iteration/v1.97`; `target_branch=main`. |
| Acceptance gate | Clean-state and existing-install desktop smoke paths are hard gates before Done. A blocker must be explicit and reproducible; unit tests alone cannot waive either smoke path. |

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-07-v1.97-desktop-first-launch-hardening` | Desktop first-launch reliability hardening | Done | Shipped verified fixes (sidecar FSM + spawn-name fix + IPC + layout + path hygiene); QC Approve with residuals; QA accept with carry-over. Hard smoke deferred: no-creator clean-state gap (R-V197-SMOKE-CLEAN-STATE, high) + headless UI smoke (R-V197-SMOKE-UI, medium) → V1.98. |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze | 2026-07-07 | done |
| Dev complete | 2026-07-08 | done |
| QC complete | 2026-07-08 | done (Approve with residuals) |
| Hard smoke complete | 2026-07-08 | deferred → V1.98 (no-creator gap + headless UI; sidecar spawn-name fix verified) |
| Iteration close | 2026-07-08 | done |

## Acceptance Criteria

- Step 1 welcome screen remains visually calm and usable at desktop window sizes: the step list does not crowd content, content remains within the card, long workspace paths truncate rather than expanding the layout, and no audited setup wizard layout regresses from V1.96.
- The native directory picker call sends the Tauri Rust command argument as `defaultPath`, not `default_path`, and Browse never fails with `command pick_directory missing required key defaultPath`.
- Current prototype/test fixtures contain no `/Users/bibi` path literals under product app code or app tests.
- `SidecarManager::new` starts from `Stopped`, not `Starting`; `start_with_budget` only short-circuits `Starting` when an owned child process is already present and being monitored. A `Starting` + no-child state is invalid and must not block a real spawn or retry.
- Clean-state desktop smoke (cleared `~/.nexus42` or equivalent isolated home/profile) records the launch command/environment, initial home/config state, observed wizard path, sidecar state transitions visible to the UI, and final result: successful wizard completion or a bounded daemon error with visible Retry/Reset/actionable copy. Indefinite "Daemon is taking longer than expected" limbo is a failure.
- Existing-install desktop smoke records the preserved config/workspace state and confirms V1.95/V1.96 behaviors still hold: setup marker, workspace path preservation/stale overwrite rules, reset-local-database recovery, and daemon diagnostics. The verification method is host-neutral; the evidence must be reproducible enough for QC/QA to inspect, but this compass does not mandate a single automation harness.
- Targeted tests for the touched web and desktop sidecar paths pass. Final plan verification records scoped commands plus both smoke transcripts, or an explicit hard-gate blocker if a smoke path cannot run.

## Non-Goals

- No new product surface, navigation area, or authoring feature.
- No schema or `@42ch/nexus-contracts` change.
- No signing, notarization, auto-update, GitHub Releases, Windows/Linux desktop distribution, tray/menu-bar, or native notification work.
- No broad config-write atomicity hardening, config-file rewrite policy, or cross-crate default workspace/path resolver consolidation.
- No rewrite of historical `.mstar` QC/QA report evidence that contains absolute review cwd paths.

## Roadmap Position

- **Current iteration (V1.97)**: `delivered` — focused hardening of the desktop first-launch sidecar lifecycle. Shipped: sidecar FSM correctness (`Stopped` initial, `Starting`+child gate, attach-without-ownership), **sidecar spawn-name fix** (`sidecar("nexus42")` per Tauri v2 — a latent first-launch blocker since V1.66), folder-picker IPC casing, setup-wizard layout containment, path hygiene. QC Approve with residuals; QA accept with carry-over.
- **Next iteration (V1.98)**: `Desktop Clean-State First-Launch Completion` — resolve the no-creator clean-state bootstrap gap (R-V197-SMOKE-CLEAN-STATE, high): the daemon requires an active creator to boot and the desktop wizard has no creator-bootstrap invoke; `.setup()` auto-starts the daemon unconditionally on launch. Needs architect design (gate daemon auto-start behind `setup_completed` + add creator-bootstrap to the wizard). Then run the deferred hard desktop smoke (clean-state + existing-install UI observation, R-V197-SMOKE-UI) on an interactive macOS host. Owner: project-manager. Trigger: V1.97 merged.
- **Final target**: a desktop onboarding path that does not require terminal knowledge, manual log inspection, or repeated harness hotfixes to get a new author into the app.

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `.mstar/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main @ 070e26f7` |
| `spec_integration_branch` | `iteration/v1.97` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| PM-thread prototype diff is accidentally treated as final implementation | Medium | High | Plan and compass mark it as prototype only; fullstack-dev must re-own and verify under SDD before Done. |
| Clean-state desktop smoke is difficult to automate in this host | Medium | High | Require explicit smoke evidence or document blocker; QA gate cannot be satisfied by unit tests alone. |
| Sidecar state-machine change regresses attached-daemon behavior | Low | High | Include Rust tests for new manager initial state and attach/spawn behavior; QC reviews sidecar lifecycle. |
| Layout fix creates token drift from DESIGN.md | Medium | Medium | Review chain checks specs/design tokens; implementer must keep edits scoped to setup wizard surface. |

## Iteration Workspace

| Path | Purpose |
|------|---------|
| `v1.97/README.md` | Workspace index for this iteration |
| `v1.97/guides/sidecar-startup-state-machine.md` | Iteration-scoped architecture handoff for sidecar lifecycle invariants, prototype intake, and smoke evidence |
| `v1.97/specs/` | Iteration-scoped spec drafts if the review chain chooses not to edit normative specs directly |

## Compound Round Summary

- Crystallized docs: **updated** `knowledge/architecture-patterns/daemon-ready-gate-pattern.md` (V1.97 refinements — Rules 9-12: `Stopped` initial state, `Starting`+`child.is_some()` gate, **Tauri v2 `sidecar()` filename rule**, attach-without-fabricated-ownership). Overlap-high → updated existing doc rather than creating a new one.
- New `CONCEPTS.md` entries: none (no new project-specific domain term).
- Workspace promotion: `iterations/v1.97/guides/sidecar-startup-state-machine.md` → durable rules promoted to the knowledge doc (trace left); `iterations/v1.97/README.md` kept as iteration-history snapshot.
- compound-refresh: not triggered (no stale knowledge contradicted by V1.97; the daemon-ready-gate doc was extended, not superseded).

## Iteration Retrospective (minimal)

- **What went well**: the hard smoke gate did its job — it surfaced a latent first-launch blocker (wrong sidecar identifier) that had been silently broken since V1.66 because no prior iteration had ever spawned the bundled sidecar end-to-end. The SDD per-task loop + intake discipline (T1 rejected the `index.css` token drift) kept scope tight. QC tri caught no Critical/Important.
- **What to improve**: the plan listed clean-state + existing-install desktop smoke as a hard Done gate but the execution environment is headless (no native Tauri window) — the gate was un-runnable in-session from the start. Future desktop-behavior plans should confirm an interactive/automated desktop smoke host is available before committing the hard-gate, or explicitly stage the smoke as a separate user-run step.
- **Next iteration suggestions**: V1.98 should lead with the architect design for the no-creator clean-state bootstrap (the spawn-name fix made the daemon reachable, which exposed this next gate), then run the deferred desktop smoke on an interactive host. Consider whether `.setup()` auto-starting the daemon unconditionally is the right lifecycle (vs. wizard-gated start).

# Desktop Clean-State First-Launch Bootstrap Contract

**Status:** Draft for V1.100 review chain
**Document class:** Iteration-scoped contract
**Coordinates with:** `.mstar/specs/desktop-shell.md`, `.mstar/specs/web-ui.md`, `.mstar/knowledge/architecture-patterns/daemon-ready-gate-pattern.md`

## Problem

V1.97 made the desktop sidecar path reachable but exposed the next clean-state blocker: on a fresh local home, the Tauri `.setup()` hook starts the bundled daemon before the wizard has created creator/workspace state. The daemon then exits with `No active creator`, and the wizard has no creator-bootstrap step that can make the launch path complete.

## Locked Direction

The preferred implementation direction is:

1. Gate app-launch daemon auto-start behind `setup_completed`.
2. Let the wizard persist workspace choice and bootstrap the minimum local creator/workspace state through a desktop-only Tauri IPC command before the daemon step.
3. Start or attach the daemon only after bootstrap is complete, then continue the existing daemon-ready and agent-detection steps.

The selected V1.100 architecture is a Tauri-side bootstrap command, named `ensure_setup_bootstrap` unless implementation discovers an existing equivalent, exposed through `DesktopCapabilities` and implemented in `apps/desktop/src-tauri` by reusing local config/database primitives where possible. It is desktop IPC only: it must not add JSON Schema wire contracts, daemon HTTP endpoints, or a daemon boot-without-creator mode.

The bundled `nexus42` sidecar remains the daemon process that starts after bootstrap. It should not be spawned as an opaque one-shot setup command unless the implementation proves direct Rust helper reuse is impractical; if that fallback is used, it must still run before daemon start and must not require a new daemon API. A daemon-runtime design change is explicitly blocked for V1.100 unless the Tauri-side bootstrap strategy is proven impossible and the plan is returned to architecture review.

## Product Contract

- Clean-state users must be able to double-click the desktop app, choose or accept a workspace, complete local creator/workspace bootstrap, wait for daemon readiness, complete agent detection, and reach the main UI.
- The user-visible success state is the main UI reached from the native desktop app, not merely a daemon process that can be started from a terminal or a browser-only setup path.
- Existing installs with `setup_completed=true` must keep the current per-launch daemon-ready behavior.
- Re-running setup with partial local state must be bounded and recoverable; it must not silently corrupt or discard existing workspace state.
- Failure copy must name the failed stage and give an actionable recovery path.

## Implementation Boundaries

In scope:

- Tauri setup lifecycle gating.
- A minimal desktop-only bootstrap IPC used by the setup wizard after workspace persistence and before daemon readiness.
- Setup wizard sequencing and status copy.
- Unit/component tests for clean-state and existing-install branches.
- Spec amendments to long-lived desktop/web UI specs if review chain promotes this contract.

Out of scope:

- Full Work/intake creation beyond the minimum local creator/workspace bootstrap needed for first launch.
- Cloud/platform account creation.
- New daemon API schemas.
- Signing, notarization, auto-update, tray, Windows/Linux release hardening.
- Broad config atomicity or default-path consolidation.

## Smoke Gate

The plan Done gate requires interactive macOS evidence. Automated Rust/web tests and a desktop bundle build are necessary evidence, but they cannot substitute for the native `.app` smoke.

- Clean-state smoke: isolated or cleared local home, `.app` launch, wizard completion, daemon reaches running, main UI visible.
- Existing-install smoke: pre-seeded setup-completed config, relaunch skips wizard, daemon auto-start/attach still reaches the main UI.
- Native folder picker and recovery copy are observed or explicitly not exercised with rationale.
- Evidence must record the local-state setup used, the observed wizard stages, daemon status, and the final UI state for both clean-state and existing-install paths.

`R-V197-SMOKE-CLEAN-STATE` must remain open until clean-state smoke passes. `R-V197-SMOKE-UI` must remain open unless both smoke paths are observed or PM/user explicitly re-scope the residual with rationale.

## Verification Strategy

- Rust desktop tests must pin three lifecycle branches: `setup_completed=false` does not auto-start the sidecar from `.setup()`, `setup_completed=true` preserves current auto-start/attach behavior, and bootstrap failure leaves setup incomplete with a recoverable error.
- Web tests must pin the wizard order: workspace persistence succeeds, bootstrap runs before the daemon step calls `startDaemon`, and browser mode remains safe when desktop capabilities are unavailable.
- Contract verification is local to Tauri IPC and React capability typing. `wire_contracts_changed` remains `false`; any proposal to touch `schemas/` blocks implementation and returns to architect review.

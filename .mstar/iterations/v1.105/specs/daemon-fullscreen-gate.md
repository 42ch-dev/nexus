# Daemon Fullscreen Gate (V1.105 P0)

**Status:** architect-locked (Phase 1 §5.2); writing-polished (§5.3)  
**Plan:** `2026-07-10-v1.105-daemon-fullscreen-gate`  
**Compass:** [`v1.105-delivery-compass.md`](../../v1.105-delivery-compass.md)  
**Tier:** Must (P0)  
**Wire:** `wire_contracts_changed: false`

## Goal

Treat local daemon readiness as an **application-level fullscreen launch requirement** — not a wizard step. Desktop **always** auto-starts the sidecar (D2). Authors never reach `/setup` or main UI until Ready (or an explicit timeout/retry/recovery path).

## Author-facing outcome

| Launch context | Author sees |
|----------------|-------------|
| Cold start (`setup_completed` false/absent) | Fullscreen **Starting daemon…** → Ready → Setup Wizard at `/setup` |
| Return visit (`setup_completed` true) | Fullscreen wait → Ready → main UI |
| Settings Re-run Setup (V1.103 R1) | Confirm clears marker → fullscreen wait → Ready → Setup Wizard (new IA — P1) |

**Key product invariant:** Daemon is **never** a numbered wizard step. The splash/gate owns startup; the wizard owns Agent + Workspace + Done only.

## User stories

- *As a new author*, I wait on a fullscreen splash while the app starts the daemon — I do not click through a "Daemon" wizard step.
- *As a returning author*, every launch waits for daemon health before the main UI — consistent with first launch.
- *As an author who re-runs setup*, I still pass the fullscreen gate before the wizard opens (marker clear does not skip daemon readiness).

## Module ownership (architect-locked)

| Layer | Owner file(s) | Responsibility |
|-------|---------------|----------------|
| Sidecar auto-start | `apps/desktop/src-tauri/src/lib.rs` — `.setup()` closure (~697–711) | **Always** `manager.start(&handle)` on launch; remove `if read_setup_completed().unwrap_or(false)` gate and update module doc (lines ~16–17). |
| Sidecar runtime | `apps/desktop/src-tauri/src/sidecar.rs` — `SidecarManager` | Unchanged spawn/attach; P0 only changes **when** `.setup()` invokes `start`. |
| Outer launch gate | **New** `apps/web/src/components/setup/daemon-launch-gate.tsx` | Desktop: fullscreen wait until Ready; browser: instant pass (`daemonReady` default `true` when `!desktop`). Wraps **all** routes in `App.tsx`. |
| Splash UI | `apps/web/src/components/setup/daemon-ready-splash.tsx` | Fullscreen chrome; P0 **extends** with diagnostic affordances migrated from `setup-step-daemon.tsx` (25s timeout, retry, `resetLocalDatabase` recovery copy). |
| Setup marker routing | `apps/web/src/components/setup/setup-gate.tsx` | **After** outer gate passes: `!completed` → `<Navigate to="/setup" />`; `completed` → `children`. **Remove** splash/subscribe logic (moved to `DaemonLaunchGate`). |
| Route wiring | `apps/web/src/App.tsx` | `SetupCompletedProvider` → `DaemonLaunchGate` → `Routes`. `/setup` and `SetupGate`-wrapped main shell are **siblings** under `DaemonLaunchGate`. |
| Wizard daemon step (retire) | `apps/web/src/pages/setup-step-daemon.tsx` | **Not** a wizard step after P1; P0 migrates wait/recovery UX to gate; file deleted or demoted in P1. **Must not** remain the clean-state `startDaemon` happy path. |
| Setup marker IPC | `apps/web/src/lib/setup-completed-context.tsx`, `apps/web/src/lib/nexus/desktop-capabilities.ts` | Unchanged read/write of `setup_completed`. |
| Re-run entry | `apps/web/src/pages/settings/settings-setup-section.tsx` | R1 unchanged: `setCompleted(false)` → `navigate('/setup')`; gate intercepts before wizard mounts. |

## SetupGate sequencing (normative)

```
App open (desktop)
  → Tauri .setup() always start sidecar          [lib.rs]
  → DaemonLaunchGate: splash until Ready         [daemon-launch-gate.tsx + daemon-ready-splash.tsx]
       ├─ route /setup → SetupWizardPage         [only after Ready — P1 mounts Agent]
       └─ route /* (main) → SetupGate
            ├─ !setup_completed → Navigate /setup
            └─ setup_completed → RootLayout + children
```

**Invariants:**

1. `DaemonLaunchGate` runs **before** either `/setup` or main shell renders on desktop.
2. `SetupGate` **never** shows `DaemonReadySplash` post-P0.
3. Happy path **never** calls `desktop.startDaemon()` — sidecar start is `.setup()` only. Gate may call `startDaemon` only in explicit recovery branches if architect-approved in implement (default: retry via reload + reset-local-database; prefer parity with V1.96/V1.97 splash error paths).
4. Agent scan (`POST /v1/daemon/agent-host/scan`) is **downstream** in P1 `SetupStepAgent` — only after this gate reaches Ready.

## Tauri `.setup()` change (D2)

**Before (V1.100 Rule 13):** `read_setup_completed().unwrap_or(false)` gates `manager.start`.

**After (V1.105):** unconditional async spawn of `manager.start(&handle)` on every app launch.

**Tests to rewrite:** `apps/desktop/src-tauri/src/lib.rs` module tests `setup_completed_absent_means_no_auto_start` / `setup_completed_true_preserves_auto_start_behavior` (~1209–1242) → assert **always-start** regardless of marker. Config roundtrip tests for `setup_completed` IPC remain.

## Splash / diagnostic migration

Source of truth for recovery UX today: `apps/web/src/pages/setup-step-daemon.tsx` (subscription, 25s timeout, `resetLocalDatabase`, error `detail` verbatim).

**Migrate to:** `daemon-ready-splash.tsx` (+ optional shared hook `useDaemonReadyWait` colocated in `daemon-launch-gate.tsx`).

**Do not migrate:** wizard Back/Continue chrome, step copy ("Start the daemon"), or `onNext` advance — those retire with the Daemon wizard step.

## Agent scan boundary (reference only)

Scan stays on existing daemon endpoint. Five normative constraints unchanged — see `.mstar/specs/desktop-shell.md` §14.3 and knowledge `architecture-patterns/local-environment-scan-safety-boundary.md` (compound at Phase 3; **do not** add knowledge in Phase 1). P0 does **not** add Tauri PATH probe (grill-me **A** rejected).

## Test seams

| Area | File | Coverage expectation |
|------|------|---------------------|
| Tauri always-start | `apps/desktop/src-tauri/src/lib.rs` `#[cfg(test)]` | Marker false/absent/true all trigger start path (unit-level gate removal). |
| Outer gate | `apps/web/src/components/setup/daemon-launch-gate.test.tsx` (new) | First-launch + return visit wait; browser instant pass; error/retry surfaces. |
| Setup marker routing | `apps/web/src/components/setup/setup-gate.test.tsx` | Post-ready: incomplete → `/setup`; complete → children; **no** splash assertions. |
| Re-run path | `apps/web/src/pages/settings/settings-setup-section.test.tsx` | Marker clear + navigate `/setup`; gate ordering covered by integration or gate unit test with mock. |
| Wizard daemon retire | `apps/web/src/pages/setup-step-daemon.test.tsx` | Delete or shrink in P1 when step removed; P0 ensures no regression in gate tests. |

## Non-Goals

- Wizard IA reorder (P1)
- Portrait chrome (P2)
- Tauri-side PATH agent scan (grill-me **A** rejected)
- Schema / wire contract changes
- Changing V1.103 Re-run Setup data semantics (marker clear only)

## Acceptance

1. Clean-state (`setup_completed` false/absent) still auto-starts sidecar (D2).
2. Fullscreen wait precedes wizard **and** main UI on every launch.
3. `setup_completed` marker still prevents main UI before wizard finish.
4. Re-run Setup path passes fullscreen gate before `/setup`.
5. Automated tests cover auto-start + gate sequencing.
6. Wizard does **not** own sidecar start on clean-state happy path after P0 lands (P1 removes Daemon step).

## Related masters (normative deltas §5.2)

- `.mstar/specs/desktop-shell.md` §13.10 — first-launch / sidecar auto-start + gate layering
- `.mstar/specs/web-ui.md` §29.13.1 — `DaemonLaunchGate` + `SetupGate` split
- Knowledge Rule 13 (compound at Phase 3 close; do not add knowledge in Phase 1)

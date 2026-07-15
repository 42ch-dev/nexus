# V1.105 Iteration Workspace

Iteration-scoped contracts and guides for **V1.105 — First-Launch Wizard Reshape**.

**Compass:** [`v1.105/delivery-compass.md`](../v1.105/delivery-compass.md)

## Story

Desktop first-launch separates **daemon readiness** (fullscreen launch gate) from **author choices** (three-step wizard):

| Phase | What the author experiences |
|-------|----------------------------|
| Launch gate (P0) | Fullscreen wait while sidecar auto-starts → Ready |
| Setup wizard (P1) | Agent → Workspace → Done (no Welcome, no Daemon step) |
| Shell (P2) | Fixed portrait card + top horizontal Steps |

**Iteration complete when:** P0–P2 automated gates pass.

| Tier | Plans | Iteration incomplete if missing? |
|------|-------|----------------------------------|
| **Must** | P0 fullscreen Daemon gate; P1 IA reorder; P2 portrait shell | **Yes** |

## Specs

| Path | Tier | Purpose |
|------|------|---------|
| `specs/daemon-fullscreen-gate.md` | Must / P0 | D2 always auto-start + fullscreen Ready gate (not a wizard step) |
| `specs/wizard-ia-reorder.md` | Must / P1 | Agent → Workspace → Done; re-run Setup compatibility |
| `specs/portrait-wizard-shell.md` | Must / P2 | H1 portrait card + N1 top Steps |

## Guides

| Path | Purpose |
|------|---------|
| `guides/studio-first-visual-then-app.md` | Studio → visual accept → App wiring (P2 required; P0/P1 as needed) |

## Locks (summary — architect §5.2)

- **Daemon:** `lib.rs` `.setup()` always auto-starts sidecar (D2); **outer** `DaemonLaunchGate` fullscreen wait before any route; **inner** `SetupGate` routes by marker.
- **Marker:** `setup_completed` still gates main UI after Ready.
- **Wizard steps:** `agent` → `workspace` → `done` (`setup-wizard-page.tsx`); retire `welcome` + `daemon` modules.
- **Agent scan:** `useScanAgents` → `POST /v1/daemon/agent-host/scan` after Ready; §14.3 five constraints; no Tauri PATH probe.
- **Workspace:** `setup-step-workspace.tsx`; default `~/Documents/nexus/default` + Browse; bootstrap on Workspace Continue.
- **Shell (P2):** 480×`min(720px,85vh)` portrait card; `TopStepIndicator`; Studio fixtures first.
- **Re-run Setup:** R1 marker clear only → gate → Agent step.
- **Wire:** `wire_contracts_changed: false`.
- **Plan order:** P0 → P1 → P2.

## Master spec amendments (§5.2)

- `.mstar/specs/desktop-shell.md` §13.10 — gate layering + D2 file ownership
- `.mstar/specs/web-ui.md` §29.13 — `DaemonLaunchGate` / wizard IA / portrait tokens

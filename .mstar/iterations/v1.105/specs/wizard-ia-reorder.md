# Wizard IA Reorder (V1.105 P1)

**Status:** IA contract locked (Task 1 confirm); architect-locked (Phase 1 §5.2); writing-polished (§5.3)  
**Plan:** `2026-07-10-v1.105-wizard-ia-reorder`  
**Compass:** [`v1.105/delivery-compass.md`](../../v1.105/delivery-compass.md)  
**Depends on:** [`daemon-fullscreen-gate.md`](daemon-fullscreen-gate.md)  
**Tier:** Must (P1)  
**Wire:** `wire_contracts_changed: false`

## Goal

Reorder first-launch wizard to **Agent → Workspace → Done**. Remove Welcome. Remove Daemon as a wizard step (owned by P0 fullscreen gate).

## Author-facing outcome

After fullscreen Ready (P0), `/setup` opens on **Agent** selection:

1. **Agent** — scan lists installed ACP agents; pick one or enter custom launch command → Continue.
2. **Workspace** — default `~/Documents/nexus/default` shown; Browse optional on desktop → Continue runs bootstrap → advances.
3. **Done** — confirm → persist agent profile + `setup_completed` → `/works`.

**Removed from author-visible flow:** Welcome greeting; Daemon startup step.

## Step machine (architect-locked)

| # | Step ID | Label (author-facing) | Module | Owns |
|---|---------|----------------------|--------|------|
| 1 | `agent` | Agent | `apps/web/src/pages/setup-step-agent.tsx` | `useScanAgents` + `AgentPicker` + custom launch command |
| 2 | `workspace` | Workspace | **New** `apps/web/src/pages/setup-step-workspace.tsx` | Default path + Browse + `ensureSetupBootstrap` on **Continue** |
| 3 | `done` | Done | `apps/web/src/pages/setup-step-done.tsx` | Confirm + `finish()` → `setAgentProfile` + `markCompleted` → `/works` |

**Orchestrator:** `apps/web/src/pages/setup-wizard-page.tsx`

```typescript
// Architect-locked type (replaces today's four-step union)
export type WizardStep = 'agent' | 'workspace' | 'done';
const [step, setStep] = useState<WizardStep>('agent');
```

**Removed step IDs:** `welcome`, `daemon` — delete from `WizardStep`, `StepIndicator` / `TopStepIndicator` (P2), and Studio `WizardStepId`.

**Retired modules:**

| File | Disposition |
|------|-------------|
| `setup-step-welcome.tsx` | Delete after workspace extraction; tests migrate to `setup-step-workspace.test.tsx`. |
| `setup-step-daemon.tsx` | Delete as wizard step; wait/recovery already on P0 gate. Tests shrink or delete in P1. |

## File ownership & bootstrap order

| Action | When | IPC / API |
|--------|------|-----------|
| Agent scan | Agent step mount (post-P0 Ready) | `useScanAgents({ filter: 'all', registry_refresh: true })` → `POST /v1/daemon/agent-host/scan` via `apps/web/src/api/queries.ts` |
| Agent Continue | Author selects agent or custom command | Advance to `workspace` only — **no** bootstrap |
| Workspace load | Workspace step mount | `desktop.getWorkspaceRoot()` with fallback `~/Documents/nexus/default` (from former Welcome) |
| Workspace Browse | Author clicks Browse | `desktop.pickDirectory(defaultPath)` |
| Workspace Continue | Author clicks Continue | 1) `setWorkspacePath` when `shouldPersistWorkspacePath` (preserve stale-pattern rules from Welcome) 2) `ensureSetupBootstrap` 3) advance to `done` |
| Done Finish | Author clicks Open Nexus | `setAgentProfile` + `markCompleted` / `setSetupCompleted(true)` → `navigate('/works')` |

**Bootstrap timing lock:** `ensureSetupBootstrap` runs **only** on Workspace **Continue** — not on Agent Continue, not on app open, not on Welcome (retired).

### R-V1105P0-001 timing confirmation (P0 residual → P1)

P0 ships D2 auto-start + `DaemonLaunchGate` and **blocks** `/setup` until Ready. That leaves clean-state bootstrap still on Welcome Continue today — intentional deferral tracked as **R-V1105P0-001**.

| Phase | When | What runs |
|-------|------|-----------|
| P0 (shipped) | App open / Re-run → gate | D2 `startDaemon` + Ready; wizard not shown until Ready |
| P1 (this plan) | Workspace **Continue** (after gate Ready) | `setWorkspacePath` (when persist rules say so) → `ensureSetupBootstrap` → advance to `done` |

**Contract:** Bootstrap is **after** gate Ready and **on** Workspace Continue — never before Ready, never on Agent Continue. Code move lands in Task 2 (`setup-step-workspace.tsx`); residual closes when that extraction ships.

## Back navigation (architect-locked)

| Step | Back target | Back button |
|------|-------------|---------------|
| `agent` | — | **Hidden** (first step) |
| `workspace` | `agent` | Visible (`ChevronLeft` / `aria-label="Back"`) |
| `done` | `workspace` | **Add** Back (not present today — P1 must wire `onBack`) |

Forward-only on Agent; Workspace and Done allow Back without clearing persisted agent profile or workspace path.

## Agent scan boundary

- **Endpoint:** `POST /v1/daemon/agent-host/scan` (unchanged).
- **Hook:** `useScanAgents` in `setup-step-agent.tsx` (same as Settings `settings-agent-section.tsx`).
- **Safety:** Five constraints in `.mstar/specs/desktop-shell.md` §14.3 — registry-known names only; bounded concurrency ≤4; ≤2s timeout per probe; no shell expansion; no user-supplied commands during scan. Reference: knowledge `architecture-patterns/local-environment-scan-safety-boundary.md` (read-only; no new knowledge files).
- **Forbidden:** Tauri PATH probe duplicate (grill-me **B**).

## V1.103 Re-run Setup compatibility (R1)

| V1.103 contract | V1.105 behavior |
|-----------------|-----------------|
| R1 clears `setup_completed` marker only | Unchanged — `settings-setup-section.tsx` `setCompleted(false)` |
| Navigate to `/setup` immediately after confirm | Unchanged — but `DaemonLaunchGate` (P0) runs first |
| Workspace path + agent profile **not** deleted | Unchanged |
| Confirm copy: "restarts the setup wizard from the beginning" | Still accurate — step order is Agent-first |

**Re-run landing:** Wizard `useState` initial step **`agent`** (not Welcome). Agent step **may** pre-fill from `getAgentProfile()` / existing scan — implementer preserves current profile read behavior if present; re-run does **not** require re-bootstrap unless author changes workspace and clicks Continue.

**Settings implement authority:** `.mstar/iterations/v1.103/specs/settings-setup-section.md` — V1.105 changes routing **around** R1, not R1 semantics.

## Non-Goals

- Portrait shell / top Steps chrome (P2)
- Changing scan safety boundary
- Multi-workspace switcher
- Auto-wiping workspace or agent on re-run
- `startDaemon` as wizard happy path (forbidden — P0 owns start)

## Acceptance

1. Only three visible steps in indicator and step machine.
2. Agent is first; scan works post-Ready (P0 dependency).
3. Workspace shows default + Browse; bootstrap on Workspace Continue.
4. Welcome and Daemon steps absent from UI.
5. Re-run Setup regression: marker clear → gate → Agent step; data preserved.
6. Vitest covers new flow (`setup-wizard-page.test.tsx`, `setup-step-workspace.test.tsx`).

## Related masters

- `.mstar/specs/desktop-shell.md` §13.10.3 — three-step table
- `.mstar/specs/web-ui.md` §29.13.2 — wizard IA

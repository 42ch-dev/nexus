# First-Launch Polish (V1.106 P1)

**Status:** Draft — writing-complete (§5.3); PM lock pending §5.4  
**Tier:** Must (P1) — iteration incomplete if missing  
**Plan:** `2026-07-10-v1.106-first-launch-polish`  
**Compass:** `../v1.106-delivery-compass.md`  
**Depends on:** P0 studio-first invariant (Studio-first required for chrome in this plan)

## Product outcome

First-time authors pick an agent, choose a workspace, and finish setup with chrome that matches Settings — without tall empty cards or a fake agent list in Studio.

**User-visible win:** Wizard Agent step looks and behaves like Settings Agent; Workspace and Done feel intentionally composed inside the portrait shell.

## Author journey (Must)

1. **DaemonReadySplash** (P0 fixture) → outer gate Ready  
2. **Agent** — real `AgentPicker` grid (installed badges, status dots, custom launch)  
3. **Workspace** — form cluster vertically centered in scroll body; **Continue** bottom-anchored  
4. **Done** — celebratory heading + short helper; **Open Nexus** bottom-anchored  
5. Control Room

Portrait shell **max-height** tokens from V1.105 H1 remain unchanged.

## FB-V1106-003 — Wizard AgentPicker parity

**Problem:** Studio wizard Agent step uses `AgentListBody` stub rows (`setup-wizard-chrome-fixtures.tsx`) while App uses shared `agent-picker.tsx`.

**User-visible outcome:** Studio and App Agent steps show the same card grid, loading/empty/error states, Installed badges, outbound links, and custom-launch row.

### Acceptance

- [ ] Studio wizard Agent fixture mounts `@web-setup/agent-picker` (shared `AgentPicker` module), not stub name-only rows.
- [ ] `data-testid="agent-picker"` present with `data-status` reflecting fixture prop (`loading` | `ready` | `empty` | `error`).
- [ ] Settings (`settings-agent-section.tsx`) and wizard (`setup-step-agent.tsx`) import the **same** `apps/web/src/components/setup/agent-picker.tsx` module.
- [ ] Optional `density="compact"` on shared component for wizard portrait only — **no** second picker implementation.
- [ ] Studio visual accept recorded before App “parity” claim (studio-first invariant).

### Voice & Content (examples)

| State | Element | Copy |
|-------|---------|------|
| Loading | Body | *Scanning for local ACP agents…* |
| Empty | Title | **No agents found on PATH** |
| Empty | Helper | *Install an agent or add a custom launch command below.* |
| Error | Title | **Could not scan for agents** |
| Custom launch | Label | **Custom launch command** |
| Installed | Badge | **Installed** |

## FB-V1106-004 — Workspace / Done layout

**Problem:** Portrait wizard steps show a large empty vertical band; Done copy is functional but not celebratory.

**User-visible outcome:** Workspace and Done bodies feel centered and complete; Done signals success without a wall of helper text.

### Done — Voice & Content (locked)

| Element | Pattern | Example |
|---------|---------|---------|
| Heading | Title Case + emoji after title | **You're ready 🎉** |
| Helper | One line, sentence case | *Open Nexus to start writing. You can change settings anytime.* |
| Primary CTA | Verb + noun | **Open Nexus** |
| Finishing | Present participle + ellipsis | *Finishing…* |

### Workspace

- [ ] Form cluster (default path row, Browse, helper) vertically centered within `data-testid="wizard-step-body"` scroll region.
- [ ] CTA row (`data-testid="wizard-cta-row"`) stays **bottom-anchored** (`mt-auto`); Back + Continue unchanged in behavior.
- [ ] Portrait `max-h-setup-wizard-wizard-max-height` / H1 shell tokens **unchanged**.

### Done

- [ ] Heading includes celebratory emoji adjacent to Title Case title — example: **You're ready 🎉** (emoji after title text; avoid emoji-only heading).
- [ ] Shortened sentence-case helper — target ≤ one line, e.g. *Open Nexus to start writing. You can change settings anytime.* (replaces longer “app menu” copy).
- [ ] Success icon + heading + helper stack centered in scroll body; **Open Nexus** CTA bottom-anchored.
- [ ] App `setup-step-done.tsx` matches accepted Studio fixture after visual accept.

### Non-goals

- Abandoning portrait max-height shell
- Resizing wizard card height unless a future FB promotes it

## V1.105 residual seeds (Must track)

Close or re-target with evidence in plan Review Gate Summary + `status.json`.

| ID | User impact | Target action |
|----|-------------|---------------|
| `R-V1105P2-001` | Studio/App step indicators may drift visually | Import `@web-setup/top-step-indicator`; remove Studio inline duplicate; export `WizardStep` from shared module |
| `R-V1105P2-002` | Portrait overflow regressions undetected in App | Add App integration test or accept with written rationale |
| `R-V1105P0-003` | Rare ready→error flash after timeout | Close race in `DaemonLaunchGate` or defer with repro |
| `R-V1105P0-004` | Retry/degraded paths under-tested | Add Vitest cases or accept with coverage map |
| `R-V1105P0-005` | Settings re-run path omits outer gate | Extend test mount or accept; gate covered elsewhere |
| `R-V1105P1-001` | Inherited gate test edge | Close with `R-V1105P0-005` or separate fix |

### AgentPicker `density` API (locked §5.2)

```typescript
export type AgentPickerDensity = 'default' | 'compact';

export interface AgentPickerProps {
  // ...existing props...
  /** Layout density. Omit or `'default'` for Settings; wizard may pass `'compact'`. */
  density?: AgentPickerDensity;
}
```

- Default when omitted: `'default'` — Settings and Studio Settings fixtures omit the prop.
- Wizard (`setup-step-agent.tsx`) may pass `density="compact"` only; no Settings layout fork.
- Studio import path unchanged: `@web-setup/agent-picker`.

### TopStepIndicator SSOT (locked §5.2)

| Owner | Path | Consumer |
|-------|------|----------|
| **SSOT module** | `apps/web/src/components/setup/top-step-indicator.tsx` | App `setup-wizard-page.tsx` |
| **Studio import** | `@web-setup/top-step-indicator` | `setup-wizard-chrome-fixtures.tsx` |

- Export `WizardStep = 'agent' | 'workspace' | 'done'` from the shared module; `setup-wizard-page.tsx` imports the type (no duplicate union).
- Delete inline `TopStepIndicator` in `setup-wizard-chrome-fixtures.tsx` after wiring import.
- `data-testid="top-step-indicator"` unchanged; closes `R-V1105P2-001`.

## Non-goals (locked)

- Settings Advanced nav (FB-V1106-005 — P2 Stretch)
- Package-promoting AgentPicker
- Radix Select rewrite
- Wire / schema changes

## Architecture locks (§5.2)

See compass **Architecture Locks** and sections above — `density` API, TopStepIndicator SSOT, portrait centering only, `wire_contracts_changed: false`.

**Writing-specialist note:** Done emoji placement uses existing `text-heading-24` stack spacing; no new wizard-specific DESIGN token unless visual accept reveals a gap.

## Verification hooks

- `pnpm --filter web test` — wizard Agent, Workspace, Done step tests green.
- `pnpm --filter design-studio test` — wizard chrome fixtures include AgentPicker + layout variants.
- `data-testid="agent-picker"` assertion in Studio fixture test.

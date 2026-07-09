# Setup Wizard UI Polish — V1.101 Iteration Contract

**Status:** Chrome polish contract locked (Task 1) — Studio/App implement against §8  
**Tier:** **Must (P1)** — required for iteration Must completeness; runs after P0  
**Plan:** `2026-07-09-v1.101-setup-wizard-ui-polish`  
**Closes:** `R-V1100P0SMOKE-UI-BACK`, `R-V1100P0SMOKE-UI-STEPS`, `R-V1100P0SMOKE-UI-POLL`  
**Wire:** `wire_contracts_changed: false`  
**Residuals (full IDs):** `R-V1100P0SMOKE-UI-BACK`, `R-V1100P0SMOKE-UI-STEPS`, `R-V1100P0SMOKE-UI-POLL` (short labels: BACK / STEPS / POLL)

## 1. Problem

V1.100 clean-state smoke observed three pre-existing wizard chrome issues:

| Residual | Symptom |
|----------|---------|
| `R-V1100P0SMOKE-UI-BACK` | Back button position incorrect |
| `R-V1100P0SMOKE-UI-STEPS` | Sidebar Steps indicator displays abnormally |
| `R-V1100P0SMOKE-UI-POLL` | Daemon “running” appears immediately on fast Back re-entry but delays when waiting — poll/subscription race |

## 2. Goals

1. Correct Back control placement per DESIGN.md / setup wizard layout rules.
2. Steps indicator shows correct current/completed/upcoming states without layout glitches.
3. Daemon status subscription uses a single coherent source of truth and predictable polling/update behavior (no duplicate subscriptions; no indefinite hang).

## 3. Non-Goals

- Rewriting the daemon event bus or sidecar FSM.
- New setup steps or IA changes beyond chrome polish.
- Settings shell / routes (**DF-70** — owned by P0 reuse story; not this plan).
- Schema / wire contract changes (`wire_contracts_changed: false`).
- Stretch `Select` work (P2) or AgentPicker product surface (P0).
- Treating interactive desktop smoke as an automated Done blocker.
- Changing `SetupGate` / main-banner / daemon-status-bar subscription ownership (wizard-step scope only).

## 4. Studio-first

1. Design Studio fixtures for wizard chrome: Back placement, Steps states (welcome / daemon / agent / done), and daemon status chip states (starting / running / error).
2. Visual acceptance in Studio.
3. App wiring: layout fixes + subscription/poll timing fixes with Vitest.

## 5. Poll constraint (LOCKED)

| Allowed | Forbidden |
|---------|-----------|
| Deduplicate React Query / effect subscriptions for daemon health | New global event bus or sidecar FSM rewrite |
| Adjust poll interval / refetch-on-focus / staleTime so Back re-entry and idle wait behave consistently | New Daemon API endpoints or schema fields |
| Single source of truth for “daemon running” chip in the wizard chrome | Treating Poll as a P0 AgentPicker or P2 Select task |

Poll = **subscription/timing only (P1)**. If implementers believe an event-bus rewrite is required → **hard stop** → PM/architect (violates Non-Goals).

**Task 1 confirmation:** No event-bus rewrite is required. Existing desktop APIs (`getDaemonStatus` + `onDaemonStatusChanged`) plus the browser `client.health()` fallback are sufficient; T3 only dedups / reorders timing inside `SetupStepDaemon`.

## 6. Acceptance (automated path) — blocks automated Done

- Studio fixtures for chrome states accepted.
- Vitest covers Back navigation layout hooks as applicable, Steps state mapping, and single-subscription / poll behavior.
- All three residuals (`BACK`, `STEPS`, `POLL`) closed or re-scoped with **automated** evidence.

## 7. Human smoke (separate gate) — does **not** block automated Done

Interactive desktop confirmation of Back / Steps / daemon status feel. Scheduled outside automated drive. Automated Done ≠ human smoke Done.

---

## 8. Chrome polish contract (LOCKED — Task 1)

Code read for this lock (BASE `48f4d8b1`):

| Surface | Path |
|---------|------|
| Wizard shell + Steps | `apps/web/src/pages/setup-wizard-page.tsx` (`SetupWizardPage`, `StepIndicator`) |
| Back consumers | `setup-step-daemon.tsx`, `setup-step-agent.tsx` (welcome/done have no Back) |
| Daemon status | `setup-step-daemon.tsx` effect |
| DESIGN SSOT | `DESIGN.md` § Setup Wizard Surface (V1.96) + frontmatter `setup-wizard-step` / `setup-wizard-surface` |
| Studio baseline | `apps/design-studio/src/pages/surfaces.tsx` (`SetupWizardFixture`) — incomplete vs App; T2 must extend |

### 8.1 Back placement (`R-V1100P0SMOKE-UI-BACK`)

**Normative (DESIGN.md § Primary CTA):**

- Continue / Finish lives in the **content-panel CTA footer** with max-width token `cta-primary-max-width`.
- Back is a **smaller, visually secondary** control **adjacent** to Continue, spaced by `cta-container-gap`.
- Back is **not** in the left Steps panel and **not** above the step body.

**Step visibility:**

| Step | Back |
|------|------|
| `welcome` | Absent |
| `daemon` | Present → previous = `welcome` |
| `agent` | Present → previous = `daemon` |
| `done` | Absent (Finish / Open Nexus only) |

**Current App drift (fix in T3):** CTA footer is a **vertical** stack (`flex-col`) with primary Continue full-width and Back `tertiary` + `self-start` **below** Continue. That violates DESIGN “adjacent.”

**Target layout (T2 fixture + T3 App):**

1. CTA footer pinned to the bottom of the content column (`mt-auto` on the CTA container is fine).
2. Primary Continue / Finish: `variant="primary"`, `w-full max-w-setup-wizard-surface-cta-primary-max-width`.
3. Back: same CTA container, **horizontally adjacent** to the primary control (leading Back, trailing / primary Continue — or a single row where Back sits beside the primary block). Use `cta-container-gap`.
4. Button variant: keep shipped **`tertiary`** (matches Vitest + V1.96 “smaller / tertiary”). DESIGN’s “smaller secondary” means hierarchy relative to primary, not a mandatory `variant="secondary"`.
5. Do not restore the pre-V1.96 `justify-between` full-width Back↔Continue split across the panel.

### 8.2 Steps state mapping (`R-V1100P0SMOKE-UI-STEPS`)

**Step order (frozen):** `welcome` → `daemon` → `agent` → `done` (labels: Welcome / Daemon / Agent / Done).

**State derivation (normative — matches `StepIndicator`):**

```text
currentIndex = indexOf(currentStep)
for each step at index i:
  i < currentIndex  → complete
  i === currentIndex → active
  i > currentIndex  → pending
```

| State | Circle tokens | Label tokens | a11y |
|-------|---------------|--------------|------|
| `active` | `step-circle-active-*` | `step-label-active-color` | `aria-current="step"` on the row |
| `complete` | `step-circle-complete-*` (green) | `step-label-active-color` | no `aria-current` |
| `pending` | `step-circle-pending-*` | `step-label-pending-color` | no `aria-current` |

**Layout rules:**

- Left panel width / padding / divider from `setup-wizard-surface` tokens; circles / connectors / labels from `setup-wizard-step`.
- Circle and label share one horizontal baseline per row (`flex items-center`, fixed `step-row-height`).
- Connector is **absolutely positioned** behind circles between non-final rows (not a flex sibling that pushes the label off-baseline). Token color: `step-connector`.
- Circle content: step number `1…4` (App). Checkmark-only active circles in today’s Studio fixture are **non-normative** — T2 must show numbers + all three states.

**Example matrices T2 must fixture:**

| `currentStep` | Welcome | Daemon | Agent | Done |
|---------------|---------|--------|-------|------|
| `welcome` | active | pending | pending | pending |
| `daemon` | complete | active | pending | pending |
| `agent` | complete | complete | active | pending |
| `done` | complete | complete | complete | active |

**Known App/Studio gaps for T2/T3 (not product IA changes):**

- Nested `<nav>` (shell + `StepIndicator`) — collapse to one labeled progress nav.
- Connector geometry (`left: calc(circle/2)`, height = row height) must not clip or misalign; Studio should mirror App absolute-connector pattern.
- Studio today only models `active|pending` and omits `complete` — extend fixtures before App polish.

### 8.3 Daemon status / poll (`R-V1100P0SMOKE-UI-POLL`)

**Single source of truth inside the wizard daemon step:**

| Environment | Source | Behavior |
|-------------|--------|----------|
| Desktop (`useDesktopCapabilities` present) | `getDaemonStatus()` then at most one `onDaemonStatusChanged` listener | Mount: read status once. If `running` / `degraded` → ready immediately (no subscribe). If `stopped` / `error` → `startDaemon()` then subscribe. If `starting` → subscribe. Cleanup unsubscribes on unmount / retry. |
| Browser / no desktop | One-shot `client.health()` | No interval poll loop; success → ready; failure → error. |

**Chip / copy states (content panel status region — Studio + App):**

| UI state | When | Copy / affordance |
|----------|------|-------------------|
| starting | not ready, no error | spinner + “Starting daemon…” |
| running | `ready` | “Daemon is running.” (Continue enabled) |
| error | terminal failure / timeout messaging | error text + Retry (+ Reset local database on desktop) |

Treat `degraded` as **running** for Continue enablement (current App).

**Timing expectations (T3):**

1. **Fast Back re-entry** (daemon already up): remount must call `getDaemonStatus()` first and show running without waiting for an event.
2. **Cold / still-starting wait:** one subscription delivers `running` / `degraded` / `error`; 25s timeout re-probes status once and surfaces bounded “taking longer…” messaging — no indefinite hang.
3. **Dedup:** at most one active `onDaemonStatusChanged` subscription per mounted `SetupStepDaemon`; no parallel interval health poll on the desktop path; browser path stays one-shot probe (retry via Retry / remount only).
4. **`ready` in the effect dependency list** must not leave a stale “ready” UI after Back→forward remount — remount resets local state; do not keep a second hidden poller.

**Explicit non-requirement:** No new Tauri event channel, no sidecar FSM rewrite, no schema/API fields. Escalate to PM/architect only if the above proves impossible with existing APIs.

### 8.4 Implementer handoff

| Task | Owns |
|------|------|
| T2 | Studio fixtures for §8.1 adjacency, §8.2 four-step × three-state matrices, §8.3 starting/running/error chips |
| T3 | App layout + `SetupStepDaemon` subscription/timing alignment + Vitest |
| T4 | Residual disposition with automated evidence; human smoke stays separate |

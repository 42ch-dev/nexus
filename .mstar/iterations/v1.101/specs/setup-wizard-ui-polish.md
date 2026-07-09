# Setup Wizard UI Polish — V1.101 Iteration Contract

**Status:** Architect-locked (iteration-start §5.2)  
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

## 6. Acceptance (automated path) — blocks automated Done

- Studio fixtures for chrome states accepted.
- Vitest covers Back navigation layout hooks as applicable, Steps state mapping, and single-subscription / poll behavior.
- All three residuals (`BACK`, `STEPS`, `POLL`) closed or re-scoped with **automated** evidence.

## 7. Human smoke (separate gate) — does **not** block automated Done

Interactive desktop confirmation of Back / Steps / daemon status feel. Scheduled outside automated drive. Automated Done ≠ human smoke Done.

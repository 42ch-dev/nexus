# Select Component Promotion — V1.101 Stretch Contract

**Status:** Architect-locked (iteration-start §5.2)  
**Tier:** **Stretch (P2)** — **not** Must; whole plan may defer to V1.102 without leaving Must incomplete  
**Plan:** `2026-07-09-v1.101-select-component-promotion`  
**Priority:** Stretch — starts only after P0+P1 automated paths are Done (unless PM documents capacity exception)  
**Wire:** `wire_contracts_changed: false`  
**Pattern reference:** V1.100 form-field promotion + [ui-component-promotion-workflow](../../../knowledge/architecture-patterns/ui-component-promotion-workflow.md) (knowledge; compound SSOT)

## 1. Goal

Promote presentational `Select` (trigger / value / item semantics + accessibility) into `@42ch/nexus-ui`, following the V1.100 form-field pattern: semantics-first, Studio-direct imports, Web thin wrappers.

## 2. Non-Goals

- Field groups / FormField framework.
- App form state managers, validation libraries.
- Dialogs, combobox-as-search, multi-select product patterns beyond basic Select.
- Schema changes (`wire_contracts_changed: false`).
- Must-tier setup work (P0 AgentPicker, P1 wizard chrome) — those are separate plans and must not be blocked by this Stretch.
- Settings shell (**DF-70**).
- Promoting `AgentPicker` (stays app-shared in P0).

## 3. Studio-first

1. Lock Select a11y/composition contract in this file (review chain / Execute Task 1).
2. Studio fixtures for closed/open, disabled, invalid, keyboard focus.
3. Visual acceptance → package implementation → Web wrapper / Studio consumer updates.

## 4. Package boundary (LOCKED — presentational only)

| Layer | Owns |
|-------|------|
| `@42ch/nexus-ui` `Select` | Presentational control: styling, `invalid`/`disabled`, native or Radix trigger/value/item semantics + a11y as locked in Execute Task 1 |
| `apps/web` thin wrapper / re-export | Optional re-export under `components/ui/select.tsx` per V1.100 guardrails; **no** validation, copy, or daemon data |
| Apps / Studio fixtures | Options, labels, form state, product copy |

Same as V1.100 form fields: package must not import app routing, daemon clients, or validation libraries. If the contract grows beyond presentational Select → defer or split; do not smuggle field groups into P2.

## 5. Deferral rule (Must integrity)

If P0+P1 automated paths are not Done, or iteration capacity is consumed by Must work, PM may mark this plan **Deferred** and retarget to V1.102. **Deferring P2 does not make Must incomplete** and must not be reported as a Must residual.

## 6. Acceptance (only if plan runs)

- Select exported from `@42ch/nexus-ui` with Studio-direct + Web thin wrapper strategy matching V1.100 form-field pattern.
- Package remains presentational (no app routing/daemon/validation).
- Studio fixtures + package tests cover closed/open, disabled, invalid, keyboard focus.

## 7. Human smoke

Not applicable as a Must gate for this Stretch. No interactive desktop smoke requirement for P2 automated Done.

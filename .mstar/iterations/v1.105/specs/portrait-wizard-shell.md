# Portrait Wizard Shell (V1.105 P2)

**Status:** architect-locked (Phase 1 §5.2); writing-polished (§5.3)  
**Plan:** `2026-07-10-v1.105-portrait-shell-steps`  
**Compass:** [`v1.105-delivery-compass.md`](../../v1.105-delivery-compass.md)  
**Depends on:** [`wizard-ia-reorder.md`](wizard-ia-reorder.md)  
**Tier:** Must (P2)  
**Wire:** `wire_contracts_changed: false`

## Goal

Ship a **fixed portrait** setup wizard card with **top horizontal Steps**, replacing the left step rail and content-driven height jumps from V1.95–V1.96 layouts.

## Author-facing outcome

The Setup Wizard appears as a centered portrait card — **taller than wide** — with stable height across Agent / Workspace / Done:

- Three step labels across the **top** of the card (current / completed / upcoming).
- Step content scrolls inside the card when lists are long (e.g. many detected agents).
- Primary **Continue** / **Finish** CTA stays anchored at the bottom of the content area.
- No left sidebar step rail.

## Geometry (architect-locked — H1)

| Token / rule | CSS variable | Tailwind key | Value |
|--------------|--------------|--------------|-------|
| Card max width | `--color-setup-wizard-wizard-max-width` | `max-w-setup-wizard-step-wizard-max-width` | **`480px`** (was `640px`) |
| Card max height cap | `--color-setup-wizard-wizard-max-height` | `h-setup-wizard-wizard-max-height` | **`720px`** (new) |
| Viewport cap | — (utility) | `max-h-[85vh]` on card root | **`85vh`** — combined: `h-[min(720px,85vh)]` or equivalent |
| Aspect intent | — | — | Height > width (portrait) |
| Content overflow | — | `flex-1 min-h-0 overflow-y-auto` on step body region | Content scrolls; card height fixed |
| CTA anchor | existing | `mt-auto` on `data-testid="wizard-cta-row"` | Stays at bottom of **step column** inside scroll region or sticky footer per implement (must remain reachable) |

**DESIGN.md / codegen path:** Update frontmatter `components.setup-wizard-step.wizard-max-width` → `480px`; add `wizard-max-height: "720px"`. Regenerate `tooling/design-tokens/src/tokens.css` + `tailwind.preset.ts` per repo codegen policy.

**Deprecated for wizard (do not use in P2 chrome):** `setup-wizard-surface-step-panel-width` (`208px`), `step-panel-*` padding tokens, vertical `step-connector` lines — tokens may remain in package for other surfaces but wizard layout **must not** reference them.

## Steps chrome (architect-locked — N1)

| Lock | Value |
|------|-------|
| Component | **New** `TopStepIndicator` — horizontal variant; lives in `setup-wizard-page.tsx` or `apps/web/src/components/setup/top-step-indicator.tsx` |
| Placement | Top of card, above scrollable step content |
| Left rail | **Removed** — no `<aside className="w-setup-wizard-surface-step-panel-width">` |
| Step IDs | `agent`, `workspace`, `done` (P1 machine) |
| States | `complete` / `active` / `pending` — reuse `setup-wizard-step-circle-*` and `setup-wizard-step-label-*` tokens |
| Layout | Horizontal `flex` row; optional short horizontal connector between circles (reuse `setup-wizard-step-connector` color) |
| Labels | **Agent**, **Workspace**, **Done** |

### Optional new tokens (implement if needed for top bar spacing)

| Token | Suggested value |
|-------|-----------------|
| `setup-wizard-step-top-bar-gap` | `{spacing.space-2}` |
| `setup-wizard-step-top-bar-padding-y` | `{spacing.space-4}` |

Add under `components.setup-wizard-step` in DESIGN.md only if horizontal spacing cannot be expressed with existing `wizard-padding` / `content-panel-padding-*`.

## Module ownership

| Surface | Owner | Boundary |
|---------|-------|----------|
| Token SSOT | `DESIGN.md` / `DESIGN.dark.md` frontmatter → `tooling/design-tokens/` | Shared by Studio + App |
| Visual SSOT (P2) | `apps/design-studio/src/fixtures/setup-wizard-chrome-fixtures.tsx` | Studio-local chrome only — no daemon client, no product routes |
| Product wiring | `apps/web/src/pages/setup-wizard-page.tsx` | Card shell + `TopStepIndicator` + step body slots |
| Step bodies | `setup-step-agent.tsx`, `setup-step-workspace.tsx`, `setup-step-done.tsx` | Content only — no layout chrome changes beyond flex/scroll compliance |
| Tests | `setup-wizard-page.test.tsx`, Studio fixture smoke | Assert portrait width/height classes + top step `data-step-id` |

**Studio-first contract:** Fixtures updated for three steps + agent-list overflow **before** App wiring. See [`guides/studio-first-visual-then-app.md`](../guides/studio-first-visual-then-app.md).

## User stories

- *As a new author*, the wizard card does not jump height when I move between steps — the frame stays stable.
- *As a new author*, I always see which step I am on via the top step bar.
- *As a new author with many installed agents*, I scroll inside the card without the window growing off-screen.

## Non-Goals

- Changing step order (P1)
- Fullscreen Daemon splash redesign beyond reuse (P0)
- New illustration / onboarding art system
- Settings shell layout changes

## Acceptance

1. Studio fixtures accepted for three steps + scroll overflow case (light/dark).
2. App matches Studio geometry and top Steps.
3. Left rail gone from wizard chrome.
4. Tests cover layout hooks (`max-w-setup-wizard-step-wizard-max-width` at 480px, top `data-step-id` attributes, no `w-setup-wizard-surface-step-panel-width` on wizard root).

## Related masters

- `.mstar/specs/web-ui.md` §29.13.3 — portrait shell normative summary
- `DESIGN.md` § Setup Wizard Surface — layout shell rewrite for V1.105

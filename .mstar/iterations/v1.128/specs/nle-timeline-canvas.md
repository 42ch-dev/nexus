# Spec — NLE Timeline canvas (V1.128 P1)

**Status:** product-reviewed, architect-locked, writing-hygiene done  
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master)  
**Compass:** [`../delivery-compass.md`](../delivery-compass.md) — AC-V1128-2  
**Plan:** [`2026-07-20-v1.128-p1-nle-timeline-canvas`](../../../plans/2026-07-20-v1.128-p1-nle-timeline-canvas.md)  
**Wire contracts:** `wire_contracts_changed: false`  
**Related specs:** [P3 — `@web-*` / `@42ch/nexus-ui` clarity](web-alias-clarity.md) (P1 owns `@web-canvas/*` NLE chrome; P3 labels Canvas surfaces)

## Problem

Studio Canvas Timeline fixtures are card matrices / directed when-axis samples — not an NLE-like vertically centered, horizontally scrubbable multi-track timeline. Dogfood asks for a video/animation-editor **visual metaphor** so maintainers can judge chrome before App wiring: height, infinite horizontal advance, layered subcategory tracks, and timeline-related items that can be pulled off the track onto the canvas.

## User value

**Before:** A maintainer reviews Timeline as disconnected cards or directed-axis samples — cannot judge whether the canvas reads as an NLE-style editing surface.

**After:** The maintainer scrolls/pans along a centered multi-track band in Studio, detaches one item from a track as a presentational demo, and confirms the same chrome appears in App Timeline — enough to sign off on layout and affordances without expecting a full editor.

## Normative decisions

1. **Studio-first (Must):** Ship presentational NLE Timeline chrome + fixtures in Design Studio **before** any App wiring. Studio visual AC is the gate for T3.
2. **Layout:** Timeline band is **vertically centered** in the canvas host; horizontal axis supports **infinite scrub** feel (scroll/pan along time). Author can pan horizontally and see track lanes remain centered in the host.
3. **Tracks:** Subcategory layers stack as **multiple tracks** (NLE metaphor). Track labels + lane height are presentational tokens/recipes — no new DESIGN hue without token gallery update.
4. **Pull-off (dogfood demo):** Timeline-related items expose a **detach / pull-off** affordance in Studio (fixture interaction or presentational demo — click-drag or explicit detach control both acceptable). Full React Flow DnD production parity is **not** required.
5. **App adoption:** After Studio visual AC, thin-wire App Timeline host to consume the new chrome extracts. No new Daemon routes / schemas unless Review forces (default false).

### React Flow boundary (architect-locked — Seat 2)

**No expanded React Flow DnD in V1.128.** Evidence: presentational extract contract in `apps/web/src/components/canvas/presentational/timeline-node-chrome.test.tsx:38–42` (extracts MUST NOT import `@xyflow/react`); existing RF hosts live in `apps/web/src/components/canvas/canvas-shell.tsx` and `timeline-canvas.tsx`.

| Layer | Allowed | Forbidden |
|-------|---------|-----------|
| P1 `@web-canvas/*` extracts | Presentational NLE band/tracks chrome; RF-free per boundary test | Any `@xyflow/react` import; new RF node types; `onNodesChange` for detach |
| Studio pull-off (T2) | Local fixture React state — DOM click-drag or explicit detach button | RF `onNodeDrag*` handlers; persistence; wire-backed placement |
| App adopt (T3) | Swap presentational chrome into **existing** RF hosts | New `useCanvasSurface` DnD surfaces; production pull-off persistence |

Pull-off interaction is **Studio fixture state only** for dogfood verify. App Timeline shows new chrome layout; pull-off behavior does not ship in App in V1.128.

**Wire contracts:** `wire_contracts_changed: false` (confirmed).

### Dogfood-verify scope cap (Must — do not expand in V1.128)

In scope for author verification:

- Multi-track visual (≥2 lanes with labels)
- Vertically centered band in canvas host
- Horizontal scrub/pan along time axis
- One pull-off interaction (item leaves track → visible on canvas area)
- Light + dark theme in Studio fixtures

Explicitly **out** of dogfood-verify (defer):

- Trim, ripple, roll edits
- Audio lanes / waveforms
- Snap-to-frame, playhead transport, keyboard shortcuts
- Production DnD persistence or wire-backed moment placement
- Computable-on-timeline; multi-World merge

## Acceptance (author-observable)

| ID | Author sees / does |
|----|-------------------|
| AC-V1128-2a | In Studio `/surfaces/canvas`, sees a **vertically centered** Timeline band with **≥2 labeled tracks**; can **pan/scroll horizontally** along the time axis in light and dark themes |
| AC-V1128-2b | In Studio, performs **one pull-off**: a timeline-related item moves from a track onto the canvas area (presentational interaction sufficient; no persistence required) |
| AC-V1128-2c | In App World/Work Timeline, the host loads and displays the **new band/tracks chrome** (replacing prior directed-axis card-matrix look where planned); no new wire-contract surfaces |

## Out of scope

Full NLE feature parity (trim, ripple, audio lanes); Computable-on-timeline; multi-World merge; Moment-on-wire; production editor keyboard model.

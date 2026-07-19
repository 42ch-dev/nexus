# Spec — Creation World-first IA first slice (V1.125 P2)

**Status:** product-reviewed, architect-reviewed (seat 2), writing-hygiene done (seat 3)  
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master)  
**Compass:** [`../delivery-compass.md`](../delivery-compass.md) — AC-V1125-5  
**Plan:** [`2026-07-19-v1.125-creation-world-first-ia`](../../../plans/2026-07-19-v1.125-creation-world-first-ia.md)  
**Wire contracts:** `wire_contracts_changed: false` — pure IA / nav / empty-state UX; no new routes or DTOs.

## Problem

Creator tab is feature-driven (Timeline peer group first). Worlds empty is a query dead-end when Create World API is absent. Settings and theme are split across sidebar footer and header.

## Normative decisions

1. **Creator groups:** Worlds (primary) → Works. **Remove** top-level Timeline and Work Timelines groups.
2. **Routes retained:** `/timeline` deep-link; World pick → `/worlds/:id/timeline`; Work → `/works/:id/outline`; Work Timeline via canvas/command palette.
3. **Empty CTAs (card-sized):** Primary CTA matches content card footprint on both Worlds and Works empty states.
   - **Works empty:** “Create Work” → existing `CreateWorkDialog` flow (reuse from `works-page.tsx`).
   - **Worlds empty — API present:** “Create World” → Create World flow (only when `NexusClient` exposes `createWorld`).
   - **Worlds empty — API absent (honest fallback):** Single card CTA opens **Work create** (`CreateWorkDialog`) with copy such as “Create a Work to get started — Worlds are created from your Works.” **No** disabled button, **no** query dead-end, **no** silent failure.
4. **Create World feature detect** — At runtime: `'createWorld' in client && typeof client.createWorld === 'function'` on the `NexusClient` instance. **Do not** add a stub `createWorld` to `NexusClient` in P2; worlds today are created implicitly from Works (`GET /v1/daemon/narrative/worlds` lists work-backed worlds only). When the method is absent, always use Work-create fallback.
5. **Settings:** Gear icon beside theme toggle in header; remove/demote sidebar Settings text row.
6. **Master spec pointer:** P2 updates [`web-ui.md`](../../../specs/web-ui.md) §29.17.4 — Worlds-first Creator **list-mode** sidebar only; §29.18 Canvas World-entry Timeline default is unchanged.

## Architecture notes (implementer)

| File | Note |
|------|------|
| `sidebar.tsx` | Reorder groups; remove Timeline + Work Timelines + Creator Memories (Memory move is P1) |
| `worlds-page.tsx` | Card-sized empty CTA; branch on `createWorld` presence |
| `works-page.tsx` | Extract or share `CreateWorkDialog` trigger for Worlds fallback |
| `header.tsx` | Settings gear adjacent to theme toggle |

## Acceptance (author-observable)

| ID | Author sees / does |
|----|-------------------|
| AC-V1125-5 | Creator: Worlds → Works; no Timeline peer groups; card CTAs; Worlds fallback → Work create with honest copy; Settings beside theme |

## Out of scope

Selection→submenu shell; canvas Brief/Narrative axis; agent dialog in sidebar; `POST /v1/daemon/narrative/worlds` or new Create World wire contract.

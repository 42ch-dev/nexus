# Studio-First Invariant (V1.106)

**Status:** Locked at iteration-start grill-me; §5.1 PM + §5.2 architect + §5.3 writing applied. Compass `status: locked` since §5.4 PM lock.

## Invariant

All subsequent UI work for author-facing chrome follows:

> **需求 →【design-studio ↔ DESIGN.md】组件打磨 → Real App 采用**

**Plain language:** Polish UI in Design Studio against DESIGN.md first; wire the real App only after visual acceptance.

## Rules

1. **Studio composition first** — fixtures in `apps/design-studio` using DESIGN tokens / `@42ch/nexus-ui` / transitional `@web-ui/*` as classified.
2. **Visual acceptance** — light/dark, focus-visible, Voice & Content (Title Case titles; sentence case helpers/errors/toasts), no unregistered scales — before App wiring claims.
3. **DESIGN.md** — when tokens or primitive contracts change, update root DESIGN pair in the same change set (or record keep-web with rationale).
4. **Promotion decision** — `promote` | `keep-web` | `keep-studio` | `defer` per `.mstar/knowledge/architecture-patterns/ui-component-promotion-workflow.md`.
5. **App integration last** — daemon data, routing, Tauri IPC stay in `apps/web` / `apps/desktop`.

## Visual acceptance checklist (before App claim)

- [ ] Fixture renders in light and dark without ad-hoc color overrides.
- [ ] Focus order and focus-visible ring match DESIGN.md.
- [ ] Copy follows Voice & Content (no protocol jargon in author-facing strings).
- [ ] keep-web components documented in DESIGN.md when used in author chrome.

## V1.106 Must completion (P0)

Pipeline is “complete” when setup → settings → control-room daemon chrome has DESIGN coverage + Studio fixtures + explicit keep-web/promote decisions — **not** when every primitive is in `@42ch/nexus-ui`.

**V1.106 keep-web lock:** Dialog, Tabs, Table, States — classify + DESIGN + fixture; do **not** package-promote this iteration.

## V1.106 Must completion (P1)

First-launch wizard chrome (Agent, Workspace, Done) follows studio-first: Studio fixture uses shared App modules where classified app-shared (e.g. `AgentPicker`); App matches accepted Studio.

## Related

- Compass: `../v1.106-delivery-compass.md`
- P0 spec: `../specs/studio-first-pipeline.md`
- P1 spec: `../specs/first-launch-polish.md`
- Plan: `../../plans/2026-07-10-v1.106-studio-first-pipeline.md`
- Workflow knowledge: `../../knowledge/architecture-patterns/ui-component-promotion-workflow.md`
